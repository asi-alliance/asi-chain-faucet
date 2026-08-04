use anyhow::{bail, Result};
use rand::{rng, seq::SliceRandom};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeSocket {
    pub host: String,
    pub grpc_port: u16,
    pub http_port: u16,
}

pub fn validate_deploy_id(deploy_id: &str) -> bool {
    let len = deploy_id.len();
    if len < 100 || len > 160 {
        return false;
    }
    deploy_id.chars().all(|c| c.is_ascii_alphanumeric())
}

#[derive(Deserialize)]
struct NodeStatusInterface {
    #[serde(rename = "isReady")]
    is_ready: bool,
}

async fn is_node_available(client: &Client, node: &NodeSocket) -> bool {
    let url = format!("http://{}:{}/api/status", node.host, node.http_port);
    match client.get(&url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(status) = resp.json::<NodeStatusInterface>().await {
                    return status.is_ready;
                }
            }
            false
        }
        Err(_) => false,
    }
}

pub async fn choose_random_node(nodes: &[NodeSocket]) -> Result<&NodeSocket> {
    if nodes.is_empty() {
        bail!("No available node sockets");
    }

    let client = Client::builder().timeout(Duration::from_secs(5)).build()?;

    let mut indices: Vec<usize> = (0..nodes.len()).collect();
    indices.shuffle(&mut rng());

    for &i in &indices {
        let node = &nodes[i];
        if is_node_available(&client, node).await {
            return Ok(node);
        }
    }

    bail!("No reachable nodes");
}
