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

// Prefer the validator used during preparation. Keep its chain/protocol identity
// as well, so a retired endpoint can be replaced without signing a new deploy.
#[derive(serde::Serialize, serde::Deserialize)]
struct PreparedTransfer {
    signed: Vec<u8>,
    endpoint: String,
    #[serde(default)]
    compatibility: Option<TransferCompatibility>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TransferCompatibility {
    network_id: String,
    shard_id: String,
    api_version: String,
    native_token_decimals: u32,
}

impl TransferCompatibility {
    fn from_status(status: &NodeStatus) -> Result<Self> {
        anyhow::ensure!(
            !status.network_id.is_empty() && !status.shard_id.is_empty(),
            "Missing node network or shard identity"
        );
        Ok(Self {
            network_id: status.network_id.clone(),
            shard_id: status.shard_id.clone(),
            api_version: status
                .version
                .get("api")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .context("Missing node API version")?
                .into(),
            native_token_decimals: status
                .native_token_decimals
                .context("Missing native token decimals")?,
        })
    }

    fn validate_validator(&self, status: &NodeStatus) -> Result<()> {
        let actual = Self::from_status(status)?;
        anyhow::ensure!(
            actual.network_id == self.network_id
                && actual.shard_id == self.shard_id
                && actual.api_version == self.api_version
                && actual.native_token_decimals == self.native_token_decimals,
            "Validator has incompatible network, shard, API version or token decimals"
        );
        anyhow::ensure!(
            status.is_ready == Some(true)
                && status.is_validator == Some(true)
                && status.is_read_only == Some(false),
            "Validator is not ready for submissions"
        );
        Ok(())
    }
}

async fn read_node_status(host: &str, http_port: u16) -> Result<NodeStatus> {
    let endpoint = Endpoint::parse(&format!("http://{host}:{http_port}"))?;
    // A redirect could supply a different node's identity.
    let response = reqwest::Client::builder()
        .connect_timeout(endpoint.connect_timeout())
        .timeout(endpoint.request_timeout())
        .redirect(reqwest::redirect::Policy::none())
        .build()?
        .get(format!("{endpoint}/api/status"))
        .send()
        .await?;
    anyhow::ensure!(
        response.status().is_success(),
        "Node status at {endpoint} returned HTTP {}",
        response.status()
    );
    response.json().await.context("Invalid node status")
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
        let status = read_node_status(&node.host, node.http_port).await?;
        let decimals = status
            .native_token_decimals
            .context("Missing native token decimals")?;
        anyhow::ensure!(
            decimals == self.config.native_token_decimals,
            "Configured decimals do not match node runtime"
        );
        let compatibility = TransferCompatibility::from_status(&status)?;
        anyhow::ensure!(
            compatibility.shard_id == node_cli::deploy::DEFAULT_SHARD_ID,
            "Validator shard does not match the signed deploy shard"
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
            compatibility: Some(compatibility),
        })?)
    }

    async fn submit_prepared(
        &self,
        prepared: &PreparedTransfer,
        deploy: &node_cli::SignedDeploy,
    ) -> Result<()> {
        let legacy_identity;
        let compatibility = match &prepared.compatibility {
            Some(compatibility) => compatibility,
            None => {
                // Older journals predate the identity snapshot. Their deploys
                // were signed for the default shard; the configured observer,
                // already trusted for reconciliation, supplies the chain identity.
                let status =
                    read_node_status(&self.config.observer_host, self.config.observer_http_port)
                        .await
                        .context("Invalid observer status for legacy claim recovery")?;
                legacy_identity = TransferCompatibility::from_status(&status)?;
                anyhow::ensure!(
                    legacy_identity.shard_id == node_cli::deploy::DEFAULT_SHARD_ID,
                    "Observer shard does not match the legacy signed deploy shard"
                );
                &legacy_identity
            }
        };
        // The journal stores only a gRPC address. Its HTTP port must come from
        // the current inventory, just like the ports for replacement validators.
        let validators = self
            .config
            .node_sockets
            .iter()
            .map(|node| (format!("http://{}:{}", node.host, node.grpc_port), node));
        let original: Option<(String, &crate::utils::NodeSocket)> = validators
            .clone()
            .find(|(endpoint, _)| endpoint == &prepared.endpoint);
        let replacements = validators.filter(|(endpoint, _)| endpoint != &prepared.endpoint);
        let mut first_error = None;
        for (endpoint, node) in original.into_iter().chain(replacements) {
            let attempt = async {
                anyhow::ensure!(
                    !self.config.node_sockets.iter().any(|other| {
                        other.host == node.host
                            && other.grpc_port == node.grpc_port
                            && other.http_port != node.http_port
                    }),
                    "Conflicting HTTP ports configured for {endpoint}"
                );
                let status = read_node_status(&node.host, node.http_port).await?;
                compatibility.validate_validator(&status)?;
                F1r3flyApi::new(Endpoint::parse(&endpoint)?)?
                    .submit(deploy)
                    .await?;
                Ok::<(), anyhow::Error>(())
            }
            .await;
            match attempt {
                Ok(()) => return Ok(()),
                Err(error) => {
                    warn!(%endpoint, %error, deploy_id = %deploy.id(),
                        "Validator could not accept the saved deploy");
                    first_error.get_or_insert(error);
                }
            }
        }
        Err(first_error
            .unwrap_or_else(|| anyhow::anyhow!("No configured validators"))
            .context("No compatible validator accepted the saved deploy"))
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
        self.submit_prepared(&prepared, &deploy)
            .await
            .inspect_err(|error| {
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
        sync::{
            atomic::{AtomicI64, Ordering},
            Mutex,
        },
    };

    fn validator_status() -> serde_json::Value {
        serde_json::json!({
            "version": {"api": "1.0", "node": "test"}, "address": "test",
            "networkId": "test-chain", "shardId": "root", "peers": 1, "nodes": 2,
            "minPhloPrice": "1", "nativeTokenDecimals": 18,
            "isReady": true, "isValidator": true, "isReadOnly": false
        })
    }

    fn prepared_transfer(endpoint: String) -> PreparedTransfer {
        let key = SigningKey::from_hex(&"01".repeat(32)).unwrap();
        let deploy = node_cli::UnsignedDeploy::new(
            "Nil",
            &DeployOptions::default(),
            10,
            node_cli::deploy::DEFAULT_SHARD_ID,
        )
        .unwrap()
        .sign(key.secret_key());
        PreparedTransfer {
            signed: deploy.to_bytes(),
            endpoint,
            compatibility: Some(
                TransferCompatibility::from_status(
                    &serde_json::from_value(validator_status()).unwrap(),
                )
                .unwrap(),
            ),
        }
    }

    fn legacy_transfer(prepared: &PreparedTransfer) -> Vec<u8> {
        // The previous journal format contains only these two fields.
        serde_json::to_vec(&serde_json::json!({
            "signed": prepared.signed, "endpoint": prepared.endpoint
        }))
        .unwrap()
    }

    struct TestValidator {
        socket: crate::utils::NodeSocket,
        submitted: Arc<Mutex<Vec<Vec<u8>>>>,
        servers: [tokio::task::JoinHandle<()>; 2],
    }

    async fn serve(app: Router) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (port, server)
    }

    fn protobuf_bytes(field: u8, bytes: &[u8]) -> Vec<u8> {
        let mut message = vec![(field << 3) | 2];
        let mut length = bytes.len();
        while length >= 128 {
            message.push((length as u8) | 0x80);
            length >>= 7;
        }
        message.push(length as u8);
        message.extend_from_slice(bytes);
        message
    }

    impl TestValidator {
        async fn start(status: serde_json::Value, answer: Result<String, tonic::Code>) -> Self {
            // DeployResponse.result is protobuf field 2 (a string).
            Self::start_with_response(status, answer.map(|id| protobuf_bytes(2, id.as_bytes())))
                .await
        }

        async fn start_with_response(
            status: serde_json::Value,
            answer: Result<Vec<u8>, tonic::Code>,
        ) -> Self {
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let requests = submitted.clone();
            let http = Router::new().route(
                "/api/status",
                get(move || {
                    let status = status.clone();
                    async move { Json(status) }
                }),
            );
            let grpc = Router::new().route(
                "/casper.v1.DeployService/doDeploy",
                post(move |body: axum::body::Bytes| {
                    let requests = requests.clone();
                    let answer = answer.clone();
                    async move {
                        // Capture the actual uncompressed protobuf received over gRPC.
                        assert_eq!(body[0], 0);
                        assert_eq!(
                            u32::from_be_bytes(body[1..5].try_into().unwrap()) as usize,
                            body.len() - 5
                        );
                        requests.lock().unwrap().push(body[5..].to_vec());
                        match answer {
                            Ok(message) => {
                                let mut frame = vec![0];
                                frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
                                frame.extend(message);
                                axum::http::Response::builder()
                                    .header("content-type", "application/grpc")
                                    .header("grpc-status", "0")
                                    .body(axum::body::Body::from(frame))
                                    .unwrap()
                            }
                            Err(code) => {
                                tonic::Status::new(code, "test validator response").into_http()
                            }
                        }
                    }
                }),
            );
            let (http_port, http_server) = serve(http).await;
            let (grpc_port, grpc_server) = serve(grpc).await;
            Self {
                socket: crate::utils::NodeSocket {
                    host: "127.0.0.1".into(),
                    grpc_port,
                    http_port,
                },
                submitted,
                servers: [http_server, grpc_server],
            }
        }

        fn endpoint(&self) -> String {
            format!("http://127.0.0.1:{}", self.socket.grpc_port)
        }
    }

    impl Drop for TestValidator {
        fn drop(&mut self) {
            for server in &self.servers {
                server.abort();
            }
        }
    }

    fn submission_service(nodes: &[&TestValidator]) -> NodeCliService {
        let mut config = AppConfig::from_env().unwrap();
        config.node_sockets = nodes.iter().map(|node| node.socket.clone()).collect();
        config.alerts_enabled = false;
        NodeCliService::new(config.clone(), AlertService::new(&config))
    }

    #[tokio::test]
    async fn matching_original_validator_is_preferred_over_configured_replacements() {
        let mut prepared = prepared_transfer(String::new());
        let id = node_cli::SignedDeploy::from_bytes(&prepared.signed)
            .unwrap()
            .id();
        let original = TestValidator::start(validator_status(), Ok(id.clone())).await;
        let replacement = TestValidator::start(validator_status(), Ok(id)).await;
        prepared.endpoint = original.endpoint();
        let node = submission_service(&[&replacement, &original]);
        ClaimNode::submit(&node, &serde_json::to_vec(&prepared).unwrap())
            .await
            .unwrap();
        assert_eq!(*original.submitted.lock().unwrap(), vec![prepared.signed]);
        assert!(replacement.submitted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn original_on_another_network_fails_over_with_the_saved_bytes() {
        let mut status = validator_status();
        status["networkId"] = serde_json::json!("another-chain");
        let mut prepared = prepared_transfer(String::new());
        let id = node_cli::SignedDeploy::from_bytes(&prepared.signed)
            .unwrap()
            .id();
        let original = TestValidator::start(status, Ok(id.clone())).await;
        let replacement = TestValidator::start(validator_status(), Ok(id.clone())).await;
        prepared.endpoint = original.endpoint();
        let mut node = submission_service(&[&original, &replacement]);
        node.config.observer_host = replacement.socket.host.clone();
        node.config.observer_http_port = replacement.socket.http_port;

        for saved in [
            serde_json::to_vec(&prepared).unwrap(),
            legacy_transfer(&prepared),
        ] {
            ClaimNode::submit(&node, &saved).await.unwrap();
            assert_eq!(ClaimNode::deploy_id(&node, &saved).unwrap(), id);
        }
        assert!(original.submitted.lock().unwrap().is_empty());
        assert_eq!(
            *replacement.submitted.lock().unwrap(),
            vec![prepared.signed.clone(), prepared.signed]
        );
    }

    #[tokio::test]
    async fn grpc_errors_resubmit_identical_bytes_and_id_to_compatible_replacement() {
        for code in [
            tonic::Code::Unavailable,
            tonic::Code::DeadlineExceeded,
            tonic::Code::Internal,
            tonic::Code::ResourceExhausted,
            tonic::Code::Unimplemented,
            tonic::Code::Unknown,
            tonic::Code::InvalidArgument,
        ] {
            let mut prepared = prepared_transfer(String::new());
            let id = node_cli::SignedDeploy::from_bytes(&prepared.signed)
                .unwrap()
                .id();
            let original = TestValidator::start(validator_status(), Err(code)).await;
            let replacement = TestValidator::start(validator_status(), Ok(id.clone())).await;
            prepared.endpoint = original.endpoint();
            let node = submission_service(&[&original, &replacement]);
            let saved = serde_json::to_vec(&prepared).unwrap();
            ClaimNode::submit(&node, &saved).await.unwrap();
            assert_eq!(ClaimNode::deploy_id(&node, &saved).unwrap(), id);
            for validator in [&original, &replacement] {
                assert_eq!(
                    *validator.submitted.lock().unwrap(),
                    vec![prepared.signed.clone()],
                    "{code:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn node_rejections_and_invalid_replies_try_compatible_replacement() {
        // DeployResponse.error is field 1; ServiceError.messages is also field 1.
        let rejection = protobuf_bytes(1, &protobuf_bytes(1, b"Phlo price below node minimum"));
        for response in [
            rejection,
            protobuf_bytes(2, b"unexpected deploy ID"),
            Vec::new(),
        ] {
            let original =
                TestValidator::start_with_response(validator_status(), Ok(response)).await;
            let prepared = prepared_transfer(original.endpoint());
            let id = node_cli::SignedDeploy::from_bytes(&prepared.signed)
                .unwrap()
                .id();
            let replacement = TestValidator::start(validator_status(), Ok(id.clone())).await;
            let node = submission_service(&[&original, &replacement]);
            let saved = serde_json::to_vec(&prepared).unwrap();
            ClaimNode::submit(&node, &saved).await.unwrap();
            assert_eq!(ClaimNode::deploy_id(&node, &saved).unwrap(), id);
            for validator in [&original, &replacement] {
                assert_eq!(
                    *validator.submitted.lock().unwrap(),
                    vec![prepared.signed.clone()]
                );
            }
        }
    }

    #[tokio::test]
    async fn unconfigured_legacy_endpoint_is_skipped_even_when_reachable() {
        let mut prepared = prepared_transfer(String::new());
        let id = node_cli::SignedDeploy::from_bytes(&prepared.signed)
            .unwrap()
            .id();
        let retired = TestValidator::start(validator_status(), Ok(id.clone())).await;
        prepared.endpoint = retired.endpoint();
        let replacement = TestValidator::start(validator_status(), Ok(id)).await;
        let mut node = submission_service(&[&replacement]);
        node.config.observer_host = "127.0.0.1".into();
        node.config.observer_http_port = replacement.socket.http_port;
        ClaimNode::submit(&node, &legacy_transfer(&prepared))
            .await
            .unwrap();
        assert!(retired.submitted.lock().unwrap().is_empty());
        assert_eq!(
            *replacement.submitted.lock().unwrap(),
            vec![prepared.signed]
        );
    }

    #[tokio::test]
    async fn legacy_recovery_without_verified_observer_identity_never_submits() {
        let observer = TestValidator::start(serde_json::Value::Null, Ok("unused".into())).await;
        let original = TestValidator::start(validator_status(), Ok("unused".into())).await;
        let replacement = TestValidator::start(validator_status(), Ok("unused".into())).await;
        let prepared = prepared_transfer(original.endpoint());
        let mut node = submission_service(&[&original, &replacement]);
        node.config.observer_host = observer.socket.host.clone();
        node.config.observer_http_port = observer.socket.http_port;
        let saved = legacy_transfer(&prepared);
        let error = ClaimNode::submit(&node, &saved).await.unwrap_err();
        assert!(error
            .to_string()
            .contains("Invalid observer status for legacy claim recovery"));
        assert!(original.submitted.lock().unwrap().is_empty());
        assert!(replacement.submitted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn incompatible_or_unready_validators_never_receive_the_saved_deploy() {
        for (field, value) in [
            ("networkId", serde_json::json!("another-chain")),
            ("shardId", serde_json::json!("another-shard")),
            ("version", serde_json::json!({"api": "2.0", "node": "test"})),
            ("nativeTokenDecimals", serde_json::json!(8)),
            ("isReady", serde_json::json!(false)),
            ("isValidator", serde_json::json!(false)),
            ("isReadOnly", serde_json::json!(true)),
            ("isReady", serde_json::Value::Null),
            ("nativeTokenDecimals", serde_json::Value::Null),
        ] {
            let mut status = validator_status();
            status[field] = value;
            let original = TestValidator::start(status.clone(), Ok("unused".into())).await;
            let replacement = TestValidator::start(status, Ok("unused".into())).await;
            let node = submission_service(&[&original, &replacement]);
            let saved = serde_json::to_vec(&prepared_transfer(original.endpoint())).unwrap();
            assert!(ClaimNode::submit(&node, &saved).await.is_err(), "{field}");
            assert!(original.submitted.lock().unwrap().is_empty(), "{field}");
            assert!(replacement.submitted.lock().unwrap().is_empty(), "{field}");
        }
    }

    #[tokio::test]
    async fn redirected_or_ambiguous_status_cannot_vouch_for_the_original_validator() {
        let mut prepared = prepared_transfer(String::new());
        let id = node_cli::SignedDeploy::from_bytes(&prepared.signed)
            .unwrap()
            .id();
        let mut original = TestValidator::start(serde_json::Value::Null, Ok(id.clone())).await;
        let replacement = TestValidator::start(validator_status(), Ok(id)).await;
        prepared.endpoint = original.endpoint();
        let saved = serde_json::to_vec(&prepared).unwrap();
        let location = format!(
            "http://127.0.0.1:{}/api/status",
            replacement.socket.http_port
        );
        let (port, redirect) = serve(Router::new().route(
            "/api/status",
            get(move || {
                let redirect = axum::response::Redirect::temporary(&location);
                async move { redirect }
            }),
        ))
        .await;
        original.socket.http_port = port;
        let mut node = submission_service(&[&original, &replacement]);
        ClaimNode::submit(&node, &saved).await.unwrap();

        let mut conflicting = original.socket.clone();
        conflicting.http_port = replacement.socket.http_port;
        node.config.node_sockets.insert(0, conflicting);
        ClaimNode::submit(&node, &saved).await.unwrap();
        assert!(original.submitted.lock().unwrap().is_empty());
        assert_eq!(
            *replacement.submitted.lock().unwrap(),
            vec![prepared.signed.clone(), prepared.signed]
        );
        redirect.abort();
    }

    #[tokio::test]
    async fn unverifiable_validators_leave_the_pending_reservation_intact() {
        let mut prepared = prepared_transfer(String::new());
        let id = node_cli::SignedDeploy::from_bytes(&prepared.signed)
            .unwrap()
            .id();
        let original = TestValidator::start(serde_json::Value::Null, Ok(id.clone())).await;
        let mut status = validator_status();
        status["isReadOnly"] = serde_json::json!(true);
        let replacement = TestValidator::start(status, Ok(id.clone())).await;
        prepared.endpoint = original.endpoint();
        let saved = serde_json::to_vec(&prepared).unwrap();
        let mut node = submission_service(&[&original, &replacement]);
        let (port, observer) = serve(Router::new().route(
            "/api/deploy-finalization-status/{id}",
            get(|| async { Json(serde_json::json!({"state": "Pending", "rejection_count": 0})) }),
        ))
        .await;
        node.config.observer_host = "127.0.0.1".into();
        node.config.observer_http_port = port;

        let root = std::env::temp_dir().join(format!("faucet-claims-{}", uuid::Uuid::new_v4()));
        let store = crate::services::claims::ClaimStore::open(&root, "A").unwrap();
        let path = root.join("41/recipient.json"); // Hex-encoded network namespace "A".
        let journal = serde_json::to_vec(&serde_json::json!({
            "amount": "10", "deploy_id": id, "signed": saved
        }))
        .unwrap();
        std::fs::write(&path, &journal).unwrap();

        assert!(ClaimNode::submit(&node, &saved).await.is_err());
        let amount = 10u32.into();
        assert_eq!(
            store
                .claim(&node, "recipient", &amount, &amount)
                .await
                .unwrap(),
            id
        );
        assert_eq!(std::fs::read(path).unwrap(), journal);
        assert!(original.submitted.lock().unwrap().is_empty());
        assert!(replacement.submitted.lock().unwrap().is_empty());
        std::fs::remove_dir_all(root).unwrap();
        observer.abort();
    }

    #[tokio::test]
    async fn replacement_search_continues_after_metadata_and_submission_errors() {
        let original = TestValidator::start(serde_json::Value::Null, Ok("unused".into())).await;
        let prepared = prepared_transfer(original.endpoint());
        let id = node_cli::SignedDeploy::from_bytes(&prepared.signed)
            .unwrap()
            .id();
        let invalid = TestValidator::start(serde_json::Value::Null, Ok("unused".into())).await;
        let overloaded =
            TestValidator::start(validator_status(), Err(tonic::Code::ResourceExhausted)).await;
        let replacement = TestValidator::start(validator_status(), Ok(id)).await;
        let node = submission_service(&[&original, &invalid, &overloaded, &replacement]);
        ClaimNode::submit(&node, &serde_json::to_vec(&prepared).unwrap())
            .await
            .unwrap();
        assert!(original.submitted.lock().unwrap().is_empty());
        assert!(invalid.submitted.lock().unwrap().is_empty());
        assert_eq!(
            *overloaded.submitted.lock().unwrap(),
            vec![prepared.signed.clone()]
        );
        assert_eq!(
            *replacement.submitted.lock().unwrap(),
            vec![prepared.signed]
        );
    }

    #[tokio::test]
    async fn all_failed_submissions_return_an_error_and_keep_the_original_deploy() {
        let original =
            TestValidator::start(validator_status(), Err(tonic::Code::InvalidArgument)).await;
        let replacement =
            TestValidator::start(validator_status(), Err(tonic::Code::Internal)).await;
        let prepared = prepared_transfer(original.endpoint());
        let node = submission_service(&[&original, &replacement]);
        let saved = serde_json::to_vec(&prepared).unwrap();
        let id = ClaimNode::deploy_id(&node, &saved).unwrap();
        assert!(ClaimNode::submit(&node, &saved).await.is_err());
        assert_eq!(ClaimNode::deploy_id(&node, &saved).unwrap(), id);
        for validator in [&original, &replacement] {
            assert_eq!(
                *validator.submitted.lock().unwrap(),
                vec![prepared.signed.clone()]
            );
        }
    }

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
