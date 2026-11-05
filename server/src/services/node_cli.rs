use crate::config::AppConfig;
use crate::services::alerts::{AlertEvent, AlertKind, AlertService};
use crate::utils::choose_random_node;
use anyhow::{Context, Result};
use node_cli::f1r3fly_api::DeployFinalizationStatus;
use node_cli::utils::CryptoUtils;
use node_cli::vault::build_transfer_rholang;
use node_cli::{DeployOptions, Endpoint, F1r3flyApi, NodeHttp, SigningKey};
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

        let key = SigningKey::from_hex(private_key_hex)?;
        let api = F1r3flyApi::new(endpoint(&node.host, node.grpc_port)?)?;
        let pk_hex = CryptoUtils::serialize_public_key(&key.public_key(), false);
        let from_address = CryptoUtils::generate_vault_address(&pk_hex)?;
        let amount = self
            .config
            .faucet_amount
            .to_string()
            .parse()
            .context("invalid faucet amount")?;
        let rholang = build_transfer_rholang(&from_address, to_address, &amount)?;
        let phlo_limit = u64::try_from(self.config.deploy_phlo_limit)
            .context("DEPLOY_PHLO_LIMIT must be nonnegative")?;
        let signed = api
            .prepare(
                &rholang,
                &DeployOptions::default().with_phlo_limit(phlo_limit),
            )
            .await?
            .sign(key.secret_key());
        let id = signed.id();
        match api.submit(&signed).await {
            Ok(id) => Ok(id),
            Err(error) => {
                let failure = TransferSubmissionError::from_client(id, error);
                if !matches!(failure, TransferSubmissionError::Admission { .. }) {
                    self.alerts.notify(
                        AlertEvent::new(AlertKind::TransferDeployFailed, failure.to_string())
                            .with_context("to", to_address),
                    );
                }
                Err(failure.into())
            }
        }
    }

    pub async fn get_balance(&self, address: &str) -> Result<String> {
        let rholang_query = build_balance_query(address)?;
        let api = F1r3flyApi::new(endpoint(
            &self.config.observer_host,
            self.config.observer_grpc_port,
        )?)?
        .with_exploratory_retry_budget(Duration::from_secs(
            self.config.exploratory_retry_budget_sec,
        ));
        let result = api
            .exploratory_deploy(&rholang_query, None, false)
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;
        Ok(result.value()?.as_amount()?.to_string())
    }

    pub async fn get_deploy_info(&self, id: String) -> Result<DeployInfo> {
        let max_wait = self.config.deploy_max_wait_sec;
        let check_interval = self.config.deploy_check_interval_sec;
        let max_attempts = (max_wait / check_interval).max(1);

        let mut latest = None;
        for attempt in 0..max_attempts {
            let api = NodeHttp::new(endpoint(
                &self.config.observer_host,
                self.config.observer_http_port,
            )?)?;
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

fn endpoint(host: &str, port: u16) -> node_cli::Result<Endpoint> {
    Endpoint::parse(&format!("http://{host}:{port}"))
}

#[derive(Debug)]
pub enum TransferSubmissionError {
    Admission {
        deploy_id: String,
        code: String,
    },
    Unknown {
        deploy_id: String,
        cause: node_cli::Error,
    },
    Rejected {
        deploy_id: String,
        cause: node_cli::Error,
    },
}

impl TransferSubmissionError {
    fn from_client(deploy_id: String, cause: node_cli::Error) -> Self {
        match cause {
            node_cli::Error::DeployAdmissionRefused { reason, .. } => Self::Admission {
                deploy_id,
                code: reason.code().to_string(),
            },
            cause @ node_cli::Error::NodeRejected { .. } => Self::Rejected { deploy_id, cause },
            cause => Self::Unknown { deploy_id, cause },
        }
    }
    pub fn deploy_id(&self) -> &str {
        match self {
            Self::Admission { deploy_id, .. }
            | Self::Unknown { deploy_id, .. }
            | Self::Rejected { deploy_id, .. } => deploy_id,
        }
    }
}

impl std::fmt::Display for TransferSubmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission { deploy_id, code } => write!(f, "Deploy {deploy_id} not accepted ({code}); please try again later"),
            Self::Unknown { deploy_id, cause } => write!(f, "Submission outcome unknown for deploy {deploy_id}: {cause}. Check this ID before requesting another transfer"),
            Self::Rejected { deploy_id, cause } => write!(f, "Deploy {deploy_id} rejected: {cause}"),
        }
    }
}
impl std::error::Error for TransferSubmissionError {}

// Preserve the existing system-vault balance ABI while migrating the client.
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

#[cfg(test)]
mod submission_tests {
    use super::*;
    #[test]
    fn only_typed_admission_is_a_known_transient_refusal() {
        for reason in [
            node_cli::DeployAdmissionReason::PoolBusy,
            node_cli::DeployAdmissionReason::AdmissionBusy,
        ] {
            let error = TransferSubmissionError::from_client(
                "signed-id".into(),
                node_cli::Error::DeployAdmissionRefused {
                    deploy_id: "signed-id".into(),
                    reason,
                    message: "busy".into(),
                },
            );
            assert!(
                matches!(&error, TransferSubmissionError::Admission { code, .. } if code == reason.code())
            );
            assert_eq!(error.deploy_id(), "signed-id");
        }
        let error = TransferSubmissionError::from_client(
            "signed-id".into(),
            node_cli::Error::Http {
                url: "http://127.0.0.1".into(),
                status: 503,
                code: None,
                message: "unavailable".into(),
            },
        );
        assert!(matches!(error, TransferSubmissionError::Unknown { .. }));
        assert_eq!(error.deploy_id(), "signed-id");
    }
}
