use crate::utils::NodeSocket;
use std::env;
use std::error::Error;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

/// Wrapper for values that must never reach logs. `Debug` prints `***`,
/// so secrets stay masked in `debug!("Server config = {:?}", config)`.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub faucet_amount: u64,
    pub faucet_max_balance: u64,
    pub private_key: Option<Secret>,

    pub node_sockets: Vec<NodeSocket>,
    /// `host:http_port` list, rendered once at startup for alert context.
    /// `Arc<str>` keeps the per-request config clone free of an allocation.
    pub node_hosts_display: Arc<str>,

    pub observer_host: String,
    pub observer_grpc_port: u16,
    pub observer_http_port: u16,

    pub server_host: String,
    pub server_port: u16,

    pub deploy_max_wait_sec: u32,
    pub deploy_check_interval_sec: u32,
    pub deploy_phlo_limit: i64,

    pub alerts_enabled: bool,
    pub mattermost_webhook_url: Option<Secret>,
    pub mattermost_channel: Option<String>,
    pub mattermost_username: String,
    pub alert_throttle_sec: u64,
    pub alert_timeout_sec: u64,
    pub alert_environment: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        dotenv::dotenv().ok();

        let node_sockets = Self::load_node_sockets().unwrap_or_default();
        let node_hosts_display = Self::render_node_hosts(&node_sockets);

        Self {
            faucet_amount: Self::parse_env_or("FAUCET_AMOUNT", 1000000000000),
            faucet_max_balance: Self::parse_env_or("FAUCET_MAX_BALANCE", 2000000000000),
            private_key: Self::parse_env_secret("PRIVATE_KEY"),

            node_sockets,
            node_hosts_display,

            observer_host: env::var("OBSERVER_HOST").unwrap_or_else(|_| "localhost".to_string()),
            observer_grpc_port: Self::parse_env_or("OBSERVER_GRPC_PORT", 40452),
            observer_http_port: Self::parse_env_or("OBSERVER_HTTP_PORT", 40453),

            server_host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: Self::parse_env_or("SERVER_PORT", 8000),

            deploy_max_wait_sec: Self::parse_env_or("DEPLOY_MAX_WAIT_SEC", 6),
            deploy_check_interval_sec: Self::parse_env_or("DEPLOY_CHECK_INTERVAL_SEC", 2),
            deploy_phlo_limit: Self::parse_env_or("DEPLOY_PHLO_LIMIT", 500_000),

            alerts_enabled: Self::parse_env_or("ALERTS_ENABLED", false),
            mattermost_webhook_url: Self::parse_env_secret("MATTERMOST_WEBHOOK_URL"),
            mattermost_channel: Self::parse_env_non_empty("MATTERMOST_CHANNEL"),
            mattermost_username: Self::parse_env_non_empty("MATTERMOST_USERNAME")
                .unwrap_or_else(|| "asi-faucet".to_string()),
            alert_throttle_sec: Self::parse_env_or("ALERT_THROTTLE_SEC", 300),
            alert_timeout_sec: Self::parse_env_or("ALERT_TIMEOUT_SEC", 5),
            alert_environment: Self::parse_env_non_empty("ALERT_ENVIRONMENT")
                .unwrap_or_else(|| "unknown".to_string()),
        }
    }

    fn parse_env_non_empty(name: &str) -> Option<String> {
        env::var(name)
            .ok()
            .map(|val| val.trim().to_string())
            .filter(|val| !val.is_empty())
    }

    fn parse_env_secret(name: &str) -> Option<Secret> {
        Self::parse_env_non_empty(name).map(Secret::new)
    }

    fn render_node_hosts(sockets: &[NodeSocket]) -> Arc<str> {
        sockets
            .iter()
            .map(|node| format!("{}:{}", node.host, node.http_port))
            .collect::<Vec<_>>()
            .join(", ")
            .into()
    }

    fn load_node_sockets() -> Result<Vec<NodeSocket>, Box<dyn std::error::Error>> {
        let hosts: Vec<String> = Self::parse_str_array("NODE_HOSTS")?;
        let grpc_ports: Vec<u16> = Self::parse_str_array("NODE_GRPC_PORTS")?;
        let http_ports: Vec<u16> = Self::parse_str_array("NODE_HTTP_PORTS")?;

        if hosts.is_empty() || grpc_ports.is_empty() || http_ports.is_empty() {
            return Err(
                "NODE_HOSTS, NODE_GRPC_PORTS, and NODE_HTTP_PORTS must be set and non-empty".into(),
            );
        }

        if hosts.len() != grpc_ports.len() || hosts.len() != http_ports.len() {
            return Err(
                "NODE_HOSTS, NODE_GRPC_PORTS, and NODE_HTTP_PORTS must have the same length".into(),
            );
        }

        let sockets: Vec<NodeSocket> = hosts
            .into_iter()
            .zip(grpc_ports.into_iter())
            .zip(http_ports.into_iter())
            .map(|((host, grpc_port), http_port)| NodeSocket {
                host,
                grpc_port,
                http_port,
            })
            .collect();

        Ok(sockets)
    }

    fn parse_str_array<T>(name: &str) -> Result<Vec<T>, Box<dyn Error>>
    where
        T: FromStr,
        T::Err: Error + 'static,
    {
        let raw = env::var(name).map_err(|_| format!("{} not set", name))?;
        let trimmed = raw.trim();

        if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
            return Err(format!("{} must be an array", name).into());
        }

        let inner = &trimmed[1..trimmed.len() - 1];
        let items: Vec<T> = inner
            .split(',')
            .map(|s| s.trim().trim_matches('"').parse::<T>())
            .collect::<Result<_, _>>()?;

        Ok(items)
    }

    fn parse_env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
        env::var(name)
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(default)
    }

    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        if self.private_key.is_none() {
            return Err("PRIVATE_KEY environment variable is required".into());
        }

        if self.faucet_amount == 0 {
            return Err("FAUCET_AMOUNT must be greater than 0".into());
        }

        if self.alerts_enabled && self.mattermost_webhook_url.is_none() {
            return Err(
                "MATTERMOST_WEBHOOK_URL environment variable is required when ALERTS_ENABLED=true"
                    .into(),
            );
        }

        Ok(())
    }

    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}
