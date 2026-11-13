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

    async fn finalization_status(&self, id: &str) -> Result<DeployFinalizationStatus> {
        self.observer()?.deploy_finalization_status(id).await?
            .with_context(|| format!(
                "Observer {}:{} does not serve /api/deploy-finalization-status/{{id}} (HTTP 404); configure an observer that supports this endpoint",
                self.config.observer_host, self.config.observer_http_port
            ))
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
            let lib_status = self.finalization_status(&id).await?;

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
        // The client detail type omits `transfers`. A vault can reject a
        // transfer even when the Rholang deploy itself finalized without error.
        match api.get_deploy(id).await {
            Ok(Some(detail)) => deploy_failure(&detail),
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

fn deploy_failure(detail: &serde_json::Value) -> (Option<String>, bool) {
    let errored = detail
        .get("errored")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let system_error = detail
        .get("systemDeployError")
        .and_then(|v| v.as_str())
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned);
    let transfer_error = detail
        .get("transfers")
        .and_then(|v| v.as_array())
        .and_then(|transfers| {
            transfers
                .iter()
                .find(|transfer| transfer.get("success").and_then(|v| v.as_bool()) == Some(false))
        })
        .map(|transfer| {
            transfer
                .get("failReason")
                .and_then(|v| v.as_str())
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or("Vault rejected transfer")
                .to_owned()
        });
    (system_error.or(transfer_error), errored)
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
        api.submit(&deploy).await.inspect_err(|error| {
            self.alerts.notify(
                AlertEvent::new(AlertKind::TransferDeployFailed, error.to_string())
                    .with_context("deploy_id", deploy.id()),
            );
        })?;
        Ok(())
    }
    async fn status(&self, id: &str) -> Result<crate::services::claims::ClaimStatus> {
        use crate::services::claims::ClaimStatus;
        let status = self.finalization_status(id).await?;
        Ok(match status.state.as_str() {
            "Finalized" => {
                let block = status
                    .latest_block_hash
                    .as_ref()
                    .filter(|hash| !hash.is_empty())
                    .context("Missing finalized block hash")?;
                let (system_deploy_error, errored) =
                    self.fetch_deploy_error(id, &self.observer()?).await;
                if errored || system_deploy_error.is_some() {
                    self.alert_deploy_error(id, &status.state, &system_deploy_error);
                }
                // A vault rejection does not change canonical finality. Keep the
                // inclusion block so reconciliation still verifies the balance.
                ClaimStatus::Finalized(block.clone())
            }
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
    use axum::{
        extract::{Path, Query},
        routing::{get, post},
        Json, Router,
    };
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicI64, Ordering},
    };

    #[test]
    fn finalized_deploy_with_rejected_transfer_is_an_error() {
        let failed = serde_json::json!({
            "errored": false, "isFinalized": true, "systemDeployError": "",
            "transfers": [{"amount": "10000000000000000000000", "success": false, "failReason": "Insufficient funds"}]
        });
        assert_eq!(
            deploy_failure(&failed),
            (Some("Insufficient funds".into()), false)
        );
        let success = serde_json::json!({
            "errored": false, "isFinalized": true, "systemDeployError": "",
            "transfers": [{"amount": "10000000000000000000000", "success": true, "failReason": ""}]
        });
        assert_eq!(deploy_failure(&success), (None, false));
    }

    #[tokio::test]
    async fn reconciliation_alerts_on_vault_rejection_without_deploy_polling() {
        let (sender, mut received) = tokio::sync::mpsc::unbounded_channel();
        let app = Router::new()
            .route("/api/deploy-finalization-status/{id}", get(|| async {
                Json(serde_json::json!({"state": "Finalized", "rejection_count": 0, "latest_block_hash": "abcd"}))
            }))
            .route("/api/deploy/{id}", get(|| async {
                Json(serde_json::json!({
                    "errored": false, "systemDeployError": "",
                    "transfers": [{"success": false, "failReason": "Insufficient funds"}]
                }))
            }))
            .route("/alerts", post(move |Json(payload): Json<serde_json::Value>| {
                let sender = sender.clone();
                async move {
                    sender.send(payload).unwrap();
                    axum::http::StatusCode::OK
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = AppConfig::from_env().unwrap();
        config.observer_host = "127.0.0.1".into();
        config.observer_http_port = port;
        config.alerts_enabled = true;
        config.mattermost_webhook_url = Some(crate::config::Secret::new(format!(
            "http://127.0.0.1:{port}/alerts"
        )));
        let node = NodeCliService::new(config.clone(), AlertService::new(&config));
        let id = "a".repeat(128);
        assert!(
            matches!(ClaimNode::status(&node, &id).await.unwrap(), ClaimStatus::Finalized(block) if block == "abcd")
        );
        let payload = tokio::time::timeout(Duration::from_secs(5), received.recv())
            .await
            .unwrap()
            .unwrap();
        let text = payload["text"].as_str().unwrap();
        assert!(text.contains("Deploy failed"));
        assert!(text.contains("Insufficient funds"));
        assert!(text.contains(&id));
        server.abort();
    }

    #[tokio::test]
    async fn missing_deploy_detail_does_not_override_finalized_status() {
        let app = Router::new().route("/api/deploy-finalization-status/{id}", get(|| async {
            Json(serde_json::json!({"state": "Finalized", "rejection_count": 0, "latest_block_hash": "abcd"}))
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = AppConfig::from_env().unwrap();
        config.observer_host = "127.0.0.1".into();
        config.observer_http_port = port;
        config.alerts_enabled = false;
        let node = NodeCliService::new(config.clone(), AlertService::new(&config));
        assert!(
            matches!(ClaimNode::status(&node, &"a".repeat(128)).await.unwrap(), ClaimStatus::Finalized(block) if block == "abcd")
        );
        server.abort();
    }

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
    async fn unsupported_observer_is_an_error_instead_of_pending() {
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
        let error = ClaimNode::status(&node, "abcd").await.err().unwrap();
        assert!(error.to_string().contains("HTTP 404"));
        assert!(error
            .to_string()
            .contains("/api/deploy-finalization-status/{id}"));
        assert!(node.get_deploy_info("abcd".into()).await.is_err());
        server.abort();
    }

    #[tokio::test]
    async fn unknown_deploy_with_http_200_pending_remains_recoverable() {
        let app = Router::new().route(
            "/api/deploy-finalization-status/{id}",
            get(|Path(id): Path<String>| async move {
                assert_eq!(id.len(), 64);
                assert!(id.bytes().all(|c| c.is_ascii_hexdigit()));
                Json(serde_json::json!({"state": "Pending", "rejection_count": 0, "latest_block_hash": null}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = AppConfig::from_env().unwrap();
        config.observer_host = "127.0.0.1".into();
        config.observer_http_port = port;
        let node = NodeCliService::new(config.clone(), AlertService::new(&config));
        assert!(matches!(
            ClaimNode::status(&node, &"a".repeat(64)).await.unwrap(),
            ClaimStatus::Pending
        ));
        server.abort();
    }

    #[tokio::test]
    async fn deploy_status_rejects_errors_and_malformed_responses() {
        for (code, body) in [
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "observer unavailable",
            ),
            (axum::http::StatusCode::OK, "not JSON"),
        ] {
            let app = Router::new().route(
                "/api/deploy-finalization-status/{id}",
                get(move || async move { (code, body) }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let mut config = AppConfig::from_env().unwrap();
            config.observer_host = "127.0.0.1".into();
            config.observer_http_port = port;
            let node = NodeCliService::new(config.clone(), AlertService::new(&config));
            assert!(ClaimNode::status(&node, &"a".repeat(64)).await.is_err());
            server.abort();
        }
    }
}
