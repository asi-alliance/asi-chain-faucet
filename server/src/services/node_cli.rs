use crate::config::AppConfig;
use crate::utils::choose_random_node;
use crate::utils::NodeSocket;
use anyhow::Result;
use node_cli::{
    args::{HttpArgs, TransferArgs, WaitArgs, WalletBalanceArgs},
    commands::{check_deploy_status, transfer_deploy, wallet_balance_command},
    utils::output::DeployCompressedInfo,
};

#[derive(Clone)]
pub struct NodeCliService {
    config: AppConfig,
}

impl NodeCliService {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn transfer_funds(&self, to_address: &str, private_key: String) -> Result<String> {
        let amount = self.config.faucet_amount;
        let node_socket: &NodeSocket = choose_random_node(&self.config.node_sockets).await?;

        let args = &TransferArgs {
            to_address: to_address.to_owned(),
            amount,
            private_key: private_key,
            host: node_socket.host.clone(),
            port: node_socket.grpc_port,
            http_port: node_socket.http_port,
            bigger_phlo: true,
            propose: false,
            max_wait: 60,
            check_interval: 5,
            observer_host: Some(self.config.observer_host.clone()),
            observer_port: Some(self.config.observer_grpc_port),
        };

        let deploy_id = transfer_deploy(args)
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;

        Ok(deploy_id.to_string())
    }

    pub async fn get_balance(&self, address: &str) -> Result<String> {
        let args = WalletBalanceArgs {
            address: address.to_owned(),
            host: self.config.observer_host.clone(),
            port: self.config.observer_grpc_port,
        };

        let (balance, _meta) = wallet_balance_command(&args)
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;

        Ok(balance)
    }

    pub async fn get_deploy_info(&self, id: String) -> Result<DeployCompressedInfo> {
        let max_wait = self.config.deploy_max_wait_sec;
        let check_interval = self.config.deploy_check_interval_sec;
        let max_attempts = max_wait / check_interval;
        let observer_host = self.config.observer_host.clone();

        let args = WaitArgs {
            private_key: self.config.private_key.clone().unwrap(),
            max_attempts,
            check_interval: check_interval as u64,
            http_args: HttpArgs {
                host: observer_host.clone(),
                port: self.config.observer_http_port,
            },
            observer_host: observer_host,
            observer_grpc_port: self.config.observer_grpc_port,
        };

        let deploy_info = check_deploy_status(id.clone(), &args)
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;

        Ok(deploy_info)
    }
}
