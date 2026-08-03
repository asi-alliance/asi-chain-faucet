use crate::config::AppConfig;
use crate::utils::choose_random_node;
use anyhow::{Context, Result};
use node_cli::f1r3fly_api::DeployFinalizationStatus;
use node_cli::utils::CryptoUtils;
use node_cli::vault::{build_balance_query, build_transfer_rholang};
use node_cli::F1r3flyApi;

#[derive(Clone)]
pub struct NodeCliService {
    config: AppConfig,
}

impl NodeCliService {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn transfer_funds(&self, to_address: &str, private_key_hex: &str) -> Result<String> {
        let node = choose_random_node(&self.config.node_sockets).await?;

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
        .map_err(|e| anyhow::anyhow!("Deploy failed: {}", e))
    }

    pub async fn get_balance(&self, address: &str) -> Result<String> {
        let rholang_query = build_balance_query(address);
        let api =
            F1r3flyApi::new_readonly(&self.config.observer_host, self.config.observer_grpc_port);
        let (balance, _block_info, _cost) = api
            .exploratory_deploy(&rholang_query, None, false)
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;
        Ok(balance)
    }

    pub async fn get_deploy_info(&self, id: String) -> Result<DeployFinalizationStatus> {
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

            if lib_status.is_terminal() {
                return Ok(lib_status);
            }
            latest = Some(lib_status);

            if attempt + 1 < max_attempts {
                tokio::time::sleep(std::time::Duration::from_secs(check_interval as u64)).await;
            }
        }

        latest.ok_or_else(|| anyhow::anyhow!("no deploy status returned"))
    }
}
