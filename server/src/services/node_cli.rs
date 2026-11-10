use crate::config::AppConfig;
use crate::services::alerts::{AlertEvent, AlertKind, AlertService};
use crate::utils::choose_random_node;
use anyhow::{Context, Result};
use node_cli::f1r3fly_api::{DeployFinalizationStatus, NodeStatus};
use node_cli::vault::build_transfer_rholang;
use node_cli::{
    ConnectionConfig, DeployOptions, Endpoint, F1r3flyApi, F1r3flyConnectionManager, NodeHttp,
    SigningKey,
};
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

        let api = F1r3flyConnectionManager::new(ConnectionConfig::new(
            Endpoint::parse(&format!("http://{}:{}", node.host, node.grpc_port))?,
            Endpoint::parse(&format!("http://{}:{}", node.host, node.http_port))?,
            SigningKey::from_hex(private_key_hex)?,
        ))?;
        let rholang =
            build_transfer_rholang(&api.get_address()?, to_address, &self.config.faucet_amount)?;
        let status: NodeStatus = serde_json::from_value(
            NodeHttp::new(Endpoint::parse(&format!(
                "http://{}:{}",
                node.host, node.http_port
            ))?)?
            .get_json("/api/status")
            .await?,
        )
        .context("Invalid node status (uint256 protocol required)")?;
        let decimals = status
            .native_token_decimals
            .context("Missing native token decimals")?;
        anyhow::ensure!(
            decimals == self.config.native_token_decimals,
            "Configured decimals do not match node runtime"
        );
        let mut options =
            DeployOptions::default().with_phlo_limit(u64::try_from(self.config.deploy_phlo_limit)?);
        // The client validates the uint256 price and maximum charge before signing.
        options.phlo_price = status.min_phlo_price;
        api.deploy(&rholang, &options).await.map_err(|e| {
            self.alerts.notify(
                AlertEvent::new(AlertKind::TransferDeployFailed, e.to_string())
                    .with_context("to", to_address),
            );
            anyhow::anyhow!("Deploy failed: {}", e)
        })
    }

    pub async fn get_balance(&self, address: &str) -> Result<String> {
        let rholang_query = build_balance_query(address)?;
        let api = F1r3flyApi::new(Endpoint::parse(&format!(
            "http://{}:{}",
            self.config.observer_host, self.config.observer_grpc_port
        ))?)?
        .with_exploratory_retry_budget(Duration::from_secs(
            self.config.exploratory_retry_budget_sec,
        ));
        let result = api.exploratory_deploy(&rholang_query, None, false).await?;
        Ok(result.value()?.as_amount()?.to_string())
    }

    pub async fn get_deploy_info(&self, id: String) -> Result<DeployInfo> {
        let max_wait = self.config.deploy_max_wait_sec;
        let check_interval = self.config.deploy_check_interval_sec;
        let max_attempts = (max_wait / check_interval).max(1);

        let mut latest = None;
        for attempt in 0..max_attempts {
            let api = self.observer()?;
            let lib_status = api
                .deploy_finalization_status(&id)
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

    fn observer(&self) -> Result<NodeHttp> {
        Ok(NodeHttp::new(Endpoint::parse(&format!(
            "http://{}:{}",
            self.config.observer_host, self.config.observer_http_port
        ))?)?)
    }

    /// Read `/api/deploy/{id}` and return its `(systemDeployError, errored)`.
    /// A missing detail is not fatal: the finalization endpoint still answers.
    async fn fetch_deploy_error(&self, id: &str, api: &NodeHttp) -> (Option<String>, bool) {
        match api.get_deploy_detail(id).await {
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

// Keep the system-vault ABI and exploratory-capacity retry behavior used by
// existing observers while decoding all integer kinds as exact uint256 amounts.
fn build_balance_query(address: &str) -> node_cli::Result<String> {
    node_cli::vault::validate_address(address)?;
    Ok(format!(
        r#"new return, rl(`rho:registry:lookup`), systemVaultCh, vaultCh, balanceCh in {{
 rl!(`rho:vault:system`, *systemVaultCh) |
 for (@(_, SystemVault) <- systemVaultCh) {{
 @SystemVault!("findOrCreate", "{address}", *vaultCh) |
 for (@either <- vaultCh) {{
 match either {{
 (true, vault) => {{
 @vault!("balance", *balanceCh) |
 for (@balance <- balanceCh) {{ return!(balance) }}
 }}
 (false, _) => return!(-1)
 }}
 }}
 }}
}}"#
    ))
}
