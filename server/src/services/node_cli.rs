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

// Keep the submission endpoint with the signed bytes so the validator checked
// for denomination and fee price during preparation is also the submission node.
#[derive(serde::Serialize, serde::Deserialize)]
struct PreparedTransfer {
    signed: Vec<u8>,
    endpoint: String,
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

    async fn prepare_transfer(&self, to_address: &str) -> Result<Vec<u8>> {
        let private_key_hex = self
            .config
            .private_key
            .as_ref()
            .context("Missing signing key")?
            .expose();
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
        .context("Invalid node status")?;
        let decimals = status
            .native_token_decimals
            .context("Missing native token decimals")?;
        anyhow::ensure!(
            decimals == self.config.native_token_decimals,
            "Configured decimals do not match node runtime"
        );
        let mut options =
            DeployOptions::default().with_phlo_limit(u64::try_from(self.config.deploy_phlo_limit)?);
        // rust-dev accepts an i64 price and encodes it as 32 bytes for signing.
        options.phlo_price = status
            .min_phlo_price
            .try_into()
            .context("Node minimum Phlo price exceeds the client i64 range")?;
        let signed = api
            .api()
            .prepare(&rholang, &options)
            .await?
            .sign(SigningKey::from_hex(private_key_hex)?.secret_key());
        Ok(serde_json::to_vec(&PreparedTransfer {
            signed: signed.to_bytes(),
            endpoint: format!("http://{}:{}", node.host, node.grpc_port),
        })?)
    }

    pub async fn get_balance(&self, address: &str) -> Result<String> {
        self.balance_at(address, None).await
    }

    async fn balance_at(&self, address: &str, block: Option<&str>) -> Result<String> {
        let rholang_query = build_balance_query(address)?;
        let api = F1r3flyApi::new(Endpoint::parse(&format!(
            "http://{}:{}",
            self.config.observer_host, self.config.observer_grpc_port
        ))?)?
        .with_exploratory_retry_budget(Duration::from_secs(
            self.config.exploratory_retry_budget_sec,
        ));
        let result = api.exploratory_deploy(&rholang_query, block, false).await?;
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

impl crate::services::claims::ClaimNode for NodeCliService {
    async fn balance(&self, address: &str, block: Option<&str>) -> Result<String> {
        if let Some(block) = block {
            let observer = self.observer()?;
            let inclusion = observer.balance(address, Some(block)).await?;
            anyhow::ensure!(
                inclusion.block.hash == block && inclusion.address == address,
                "Observer returned another inclusion block"
            );
            let current = observer.balance(address, None).await?;
            anyhow::ensure!(
                current.block.number >= inclusion.block.number && current.address == address,
                "Observer finalized balance is behind claim inclusion"
            );
            return Ok(current.dust.to_string());
        }
        self.balance_at(address, None).await
    }
    async fn prepare(&self, address: &str) -> Result<Vec<u8>> {
        self.prepare_transfer(address).await
    }
    fn deploy_id(&self, signed: &[u8]) -> Result<String> {
        let prepared: PreparedTransfer = serde_json::from_slice(signed)?;
        Ok(node_cli::SignedDeploy::from_bytes(&prepared.signed)?.id())
    }
    async fn submit(&self, signed: &[u8]) -> Result<()> {
        let prepared: PreparedTransfer = serde_json::from_slice(signed)?;
        let deploy = node_cli::SignedDeploy::from_bytes(&prepared.signed)?;
        let api = F1r3flyApi::new(Endpoint::parse(&prepared.endpoint)?)?;
        api.submit(&deploy).await.map_err(|error| {
            self.alerts.notify(
                AlertEvent::new(AlertKind::TransferDeployFailed, error.to_string())
                    .with_context("deploy_id", deploy.id()),
            );
            error
        })?;
        Ok(())
    }
    async fn status(&self, id: &str) -> Result<crate::services::claims::ClaimStatus> {
        use crate::services::claims::ClaimStatus;
        let Some(status) = self.observer()?.deploy_finalization_status(id).await? else {
            // The journal may have committed immediately before a crash, without
            // any submission. Resending those exact bytes is safe even if unknown.
            return Ok(ClaimStatus::Pending);
        };
        Ok(match status.state.as_str() {
            "Finalized" => ClaimStatus::Finalized(
                status
                    .latest_block_hash
                    .filter(|hash| !hash.is_empty())
                    .context("Missing finalized block hash")?,
            ),
            "Failed" => ClaimStatus::Failed,
            "Expired" => ClaimStatus::Expired,
            _ => ClaimStatus::Pending,
        })
    }
}

#[cfg(test)]
mod claim_tests {
    use super::*;
    use crate::services::claims::{ClaimNode, ClaimStatus};
    use axum::{extract::Query, routing::get, Json, Router};
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicI64, Ordering},
    };

    #[tokio::test]
    async fn finalized_claim_rejects_lagging_observer_and_reads_later_spending() {
        let height = Arc::new(AtomicI64::new(19));
        let response_height = height.clone();
        let app = Router::new().route("/api/balance/{address}", get(
            move |Query(query): Query<HashMap<String, String>>| {
                let response_height = response_height.clone();
                async move {
                    let pinned = query.contains_key("block_hash");
                    Json(serde_json::json!({
                        "address": "1111222222222222222222222222222222222222",
                        "balance": if pinned { "10" } else { "9" },
                        "blockHash": if pinned { "abcd" } else { "ef01" },
                        "blockNumber": if pinned { 20 } else { response_height.load(Ordering::SeqCst) }
                    }))
                }
            }
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = AppConfig::from_env().unwrap();
        config.observer_host = "127.0.0.1".into();
        config.observer_http_port = port;
        let node = NodeCliService::new(config.clone(), AlertService::new(&config));
        let address = "1111222222222222222222222222222222222222";
        assert!(ClaimNode::balance(&node, address, Some("abcd"))
            .await
            .is_err());
        height.store(21, Ordering::SeqCst);
        assert_eq!(
            ClaimNode::balance(&node, address, Some("abcd"))
                .await
                .unwrap(),
            "9"
        );
        server.abort();
    }

    #[tokio::test]
    async fn unknown_deploy_status_keeps_same_deploy_recoverable() {
        let app = Router::new().route(
            "/api/deploy-finalization-status/{id}",
            get(|| async { axum::http::StatusCode::NOT_FOUND }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = AppConfig::from_env().unwrap();
        config.observer_host = "127.0.0.1".into();
        config.observer_http_port = port;
        let node = NodeCliService::new(config.clone(), AlertService::new(&config));
        assert!(matches!(
            ClaimNode::status(&node, "abcd").await.unwrap(),
            ClaimStatus::Pending
        ));
        server.abort();
    }
}
