use crate::config::AppConfig;
use crate::services::alerts::{AlertEvent, AlertKind, AlertService};
use crate::utils::choose_random_node;
use anyhow::{Context, Result};
use node_cli::f1r3fly_api::DeployFinalizationStatus;
use node_cli::utils::CryptoUtils;
use node_cli::vault::{build_balance_query, build_transfer_rholang};
use node_cli::F1r3flyApi;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, warn};

#[derive(Clone)]
pub struct NodeCliService {
    config: AppConfig,
    alerts: Arc<AlertService>,
}

/// Deploy status enriched with the node's `/api/deploy/{id}` detail so callers
/// can surface a `systemDeployError` (e.g. insufficient funds) that the
/// finalization endpoint alone does not report.
pub struct DeployInfo {
    pub finalization: DeployFinalizationStatus,
    pub system_deploy_error: Option<String>,
    pub errored: bool,
}

impl NodeCliService {
    pub fn new(config: AppConfig, alerts: Arc<AlertService>) -> Self {
        Self { config, alerts }
    }

    pub async fn transfer_funds(&self, to_address: &str, private_key_hex: &str) -> Result<String> {
        let node = match choose_random_node(&self.config.node_sockets).await {
            Ok(node) => node,
            Err(e) => {
                self.alerts.notify(
                    AlertEvent::new(AlertKind::NoReachableNodes, e.to_string())
                        .with_context("nodes", self.config.node_hosts_display.as_ref()),
                );
                return Err(e);
            }
        };

        let api = F1r3flyApi::new(private_key_hex, &node.host, node.grpc_port)
            .map_err(|e| anyhow::anyhow!("Failed to initialise deploy client: {}", e))?;

        let sk = CryptoUtils::decode_private_key(private_key_hex)
            .expect("key already validated by F1r3flyApi::new");
        let pk_hex = CryptoUtils::serialize_public_key(&CryptoUtils::derive_public_key(&sk), false);
        let from_address = CryptoUtils::generate_vault_address(&pk_hex)
            .expect("valid key always produces a vault address");

        let rholang = build_transfer_rholang(&from_address, to_address, self.config.faucet_amount);

        api.deploy_with_phlo_limit_and_expiration(
            &rholang,
            self.config.deploy_phlo_limit,
            "rholang",
            0,
        )
        .await
        .map_err(|e| {
            self.alerts.notify(
                AlertEvent::new(AlertKind::TransferDeployFailed, e.to_string())
                    .with_context("to", to_address),
            );
            anyhow::anyhow!("Deploy failed: {}", e)
        })
    }

    pub async fn get_balance(&self, address: &str) -> Result<String> {
        let rholang_query = build_balance_query(address);
        let api =
            F1r3flyApi::new_readonly(&self.config.observer_host, self.config.observer_grpc_port)
                .with_exploratory_retry_budget(Duration::from_secs(
                    self.config.exploratory_retry_budget_sec,
                ));
        let (balance, _block_info, _cost) = api
            .exploratory_deploy(&rholang_query, None, false)
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;
        Ok(balance)
    }

    pub async fn get_deploy_info(&self, id: String) -> Result<DeployInfo> {
        let max_wait = self.config.deploy_max_wait_sec;
        let check_interval = self.config.deploy_check_interval_sec;
        let max_attempts = (max_wait / check_interval).max(1);

        let mut latest = None;
        for attempt in 0..max_attempts {
            let api = F1r3flyApi::new_readonly(&self.config.observer_host, 0);
            let lib_status = api
                .deploy_finalization_status(&id, self.config.observer_http_port)
                .await
                .map_err(|e| anyhow::Error::msg(e.to_string()))?
                .context("deploy-finalization-status endpoint not available")?;

            let (system_deploy_error, errored) = self.fetch_deploy_error(&id, &api).await;

            if errored || system_deploy_error.is_some() {
                self.alert_deploy_error(&id, &lib_status.state, &system_deploy_error);
            } else if lib_status.is_terminal() && lib_status.state != "Finalized" {
                self.report_deploy_failure(&id, &lib_status);
            }

            let info = DeployInfo {
                finalization: lib_status,
                system_deploy_error,
                errored,
            };

            if info.finalization.is_terminal() {
                return Ok(info);
            }
            latest = Some(info);

            if attempt + 1 < max_attempts {
                tokio::time::sleep(std::time::Duration::from_secs(check_interval as u64)).await;
            }
        }

        latest.ok_or_else(|| anyhow::anyhow!("no deploy status returned"))
    }

    /// Read `/api/deploy/{id}` and return its `(systemDeployError, errored)`.
    /// A missing detail is not fatal: the finalization endpoint still answers.
    async fn fetch_deploy_error(&self, id: &str, api: &F1r3flyApi<'_>) -> (Option<String>, bool) {
        match api
            .get_deploy_detail(id, self.config.observer_http_port)
            .await
        {
            Ok(Some(detail)) => {
                // In previous versions node emits `systemDeployError` as an empty string on
                // success; only a non-blank message is a real failure.
                let system_deploy_error = detail
                    .system_deploy_error
                    .filter(|err| !err.trim().is_empty());
                (system_deploy_error, detail.errored)
            }
            Ok(None) => (None, false),
            Err(e) => {
                warn!("FAUCET: Failed to fetch deploy detail for {}: {}", id, e);
                (None, false)
            }
        }
    }

    /// Surface a node-side `systemDeployError` as an error log and a Mattermost
    /// alert. The alert service throttles repeats of the same kind.
    fn alert_deploy_error(&self, id: &str, state: &str, system_deploy_error: &Option<String>) {
        let reason = system_deploy_error
            .clone()
            .unwrap_or_else(|| "Deploy execution errored".to_string());

        error!("FAUCET: Deploy {} failed: {}", id, reason);
        self.alerts.notify(
            AlertEvent::new(AlertKind::DeployFailed, reason)
                .with_context("deploy_id", id)
                .with_context("state", state.to_string()),
        );
    }

    /// Raise a generic alert when finalization ends in a non-clean terminal
    /// state (`Failed`/`Expired`) and no `systemDeployError` was reported.
    fn report_deploy_failure(&self, id: &str, status: &DeployFinalizationStatus) {
        let reason = match status.state.as_str() {
            "Failed" => "Deploy execution failed".to_string(),
            "Expired" => "Deploy expired".to_string(),
            other => format!("Deploy ended in state {other}"),
        };

        error!(
            "FAUCET: Deploy {} ended in state {}: {}",
            id, status.state, reason
        );
        self.alerts.notify(
            AlertEvent::new(AlertKind::DeployFailed, reason)
                .with_context("deploy_id", id)
                .with_context("state", status.state.clone()),
        );
    }
}
