use crate::utils::NodeSocket;
use std::env;
use std::error::Error;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub faucet_amount: u64,
    pub faucet_max_balance: u64,
    pub private_key: Option<String>,

    pub node_sockets: Vec<NodeSocket>,

    pub readonly_host: String,
    pub readonly_grpc_port: u16,
    pub readonly_http_port: u16,

    pub server_host: String,
    pub server_port: u16,

    pub deploy_max_wait_sec: u32,
    pub deploy_check_interval_sec: u32,
}

impl AppConfig {
    pub fn from_env() -> Self {
        dotenv::dotenv().ok();

        Self {
            faucet_amount: Self::parse_env_or("FAUCET_AMOUNT", 10000),
            faucet_max_balance: Self::parse_env_or("FAUCET_MAX_BALANCE", 20000),
            private_key: env::var("PRIVATE_KEY").ok(),

            node_sockets: Self::load_node_sockets().unwrap_or_default(),

            readonly_host: env::var("READONLY_HOST").unwrap_or_else(|_| "localhost".to_string()),
            readonly_grpc_port: Self::parse_env_or("READONLY_GRPC_PORT", 40452),
            readonly_http_port: Self::parse_env_or("READONLY_HTTP_PORT", 40453),

            server_host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: Self::parse_env_or("SERVER_PORT", 8000),

            deploy_max_wait_sec: Self::parse_env_or("DEPLOY_MAX_WAIT_SEC", 6),
            deploy_check_interval_sec: Self::parse_env_or("DEPLOY_CHECK_INTERVAL_SEC", 2),
        }
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

        Ok(())
    }

    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}
