use crate::config::AppConfig;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

const SERVICE_NAME: &str = "asi-faucet";
const SEVERITY: &str = "critical";

/// The only events that raise an alert. Throttling is keyed on this enum, so a
/// storm of failures of the same kind collapses into a single message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlertKind {
    NoReachableNodes,
    TransferDeployFailed,
}

impl AlertKind {
    fn title(&self) -> &'static str {
        match self {
            AlertKind::NoReachableNodes => "No reachable nodes",
            AlertKind::TransferDeployFailed => "Transfer deploy failed",
        }
    }
}

/// A single alert occurrence. Carries no secret-bearing fields by construction.
#[derive(Debug)]
pub struct AlertEvent {
    kind: AlertKind,
    error: String,
    context: Vec<(&'static str, String)>,
}

impl AlertEvent {
    pub fn new(kind: AlertKind, error: impl Into<String>) -> Self {
        Self {
            kind,
            error: error.into(),
            context: Vec::new(),
        }
    }

    pub fn with_context(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.context.push((key, value.into()));
        self
    }
}

#[derive(Serialize)]
struct WebhookPayload {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<String>,
}

struct ThrottleState {
    last_sent: Instant,
    suppressed: u32,
}

pub struct AlertService {
    enabled: bool,
    webhook_url: Option<String>,
    channel: Option<String>,
    username: Option<String>,
    environment: String,
    throttle_window: Duration,
    request_timeout: Duration,
    client: Client,
    throttle: Mutex<HashMap<AlertKind, ThrottleState>>,
}

impl AlertService {
    pub fn new(config: &AppConfig) -> Arc<Self> {
        let webhook_url = config
            .mattermost_webhook_url
            .as_ref()
            .map(|url| url.expose().to_string());

        let enabled = config.alerts_enabled && webhook_url.is_some();
        if enabled {
            info!(
                "FAUCET: Alerting enabled (environment={}, throttle={}s)",
                config.alert_environment, config.alert_throttle_sec
            );
        } else {
            info!("FAUCET: Alerting is disabled");
        }

        let request_timeout = Duration::from_secs(config.alert_timeout_sec);
        let client = Client::builder()
            .timeout(request_timeout)
            .build()
            .unwrap_or_else(|e| {
                warn!("FAUCET: Failed to build alert HTTP client: {}", e);
                Client::new()
            });

        Arc::new(Self {
            enabled,
            webhook_url,
            channel: config.mattermost_channel.clone(),
            username: Some(config.mattermost_username.clone()).filter(|name| !name.is_empty()),
            environment: config.alert_environment.clone(),
            throttle_window: Duration::from_secs(config.alert_throttle_sec),
            request_timeout,
            client,
            throttle: Mutex::new(HashMap::new()),
        })
    }

    /// Fire-and-forget: never blocks the caller, never returns an error.
    /// Delivery failures are logged and nothing else.
    pub fn notify(self: &Arc<Self>, event: AlertEvent) {
        if !self.enabled {
            return;
        }

        let Some(suppressed) = self.reserve_slot(event.kind) else {
            return;
        };

        let payload = WebhookPayload {
            text: self.format_message(&event, suppressed),
            username: self.username.clone(),
            channel: self.channel.clone(),
        };

        let service = Arc::clone(self);
        tokio::spawn(async move { service.deliver(payload).await });
    }

    /// Returns the number of repeats suppressed since the last delivery, or
    /// `None` when this alert falls inside the throttle window.
    fn reserve_slot(&self, kind: AlertKind) -> Option<u32> {
        let mut throttle = self
            .throttle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        match throttle.get_mut(&kind) {
            Some(state) if state.last_sent.elapsed() < self.throttle_window => {
                state.suppressed = state.suppressed.saturating_add(1);
                None
            }
            Some(state) => {
                let suppressed = state.suppressed;
                state.last_sent = Instant::now();
                state.suppressed = 0;
                Some(suppressed)
            }
            None => {
                throttle.insert(
                    kind,
                    ThrottleState {
                        last_sent: Instant::now(),
                        suppressed: 0,
                    },
                );
                Some(0)
            }
        }
    }

    fn format_message(&self, event: &AlertEvent, suppressed: u32) -> String {
        let mut text = format!(
            ":rotating_light: **[{}] {} — {}**\n",
            self.environment,
            SERVICE_NAME,
            event.kind.title()
        );

        text.push_str(&format!("- severity: {SEVERITY}\n"));
        text.push_str(&format!("- error: {}\n", event.error));

        for (key, value) in &event.context {
            text.push_str(&format!("- {key}: {value}\n"));
        }

        text.push_str(&format!("- time: {}\n", chrono::Utc::now().to_rfc3339()));

        if suppressed > 0 {
            text.push_str(&format!(
                "- suppressed: {} repeat(s) in the previous {}s window\n",
                suppressed,
                self.throttle_window.as_secs()
            ));
        }

        text
    }

    async fn deliver(&self, payload: WebhookPayload) {
        let Some(url) = self.webhook_url.as_deref() else {
            return;
        };

        let request = self.client.post(url).json(&payload).send();

        match tokio::time::timeout(self.request_timeout, request).await {
            Err(_) => warn!("FAUCET: Alert delivery timed out"),
            Ok(Err(e)) => warn!("FAUCET: Alert delivery failed: {}", e),
            Ok(Ok(response)) if !response.status().is_success() => {
                warn!(
                    "FAUCET: Alert delivery rejected with status {}",
                    response.status()
                )
            }
            Ok(Ok(_)) => {}
        }
    }
}
