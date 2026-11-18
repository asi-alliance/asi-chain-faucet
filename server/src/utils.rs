use anyhow::{bail, Result};
use rand::{rng, seq::SliceRandom};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub(crate) use node_cli::vault::tokens_to_dust as tokens_to_units;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeSocket {
    pub host: String,
    pub grpc_port: u16,
    pub http_port: u16,
}

pub fn validate_deploy_id(deploy_id: &str) -> bool {
    let len = deploy_id.len();
    if !(100..=160).contains(&len) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, routing::get, Router};
    use tokio::{net::TcpListener, task::JoinHandle};

    struct TestNode {
        socket: NodeSocket,
        server: JoinHandle<()>,
    }

    impl TestNode {
        async fn start(status: StatusCode, body: &str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let body = body.to_owned();
            let app = Router::new().route(
                "/api/status",
                get(move || {
                    let body = body.clone();
                    async move { (status, body) }
                }),
            );
            let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            Self {
                socket: NodeSocket {
                    host: "127.0.0.1".to_string(),
                    grpc_port: 0,
                    http_port: port,
                },
                server,
            }
        }
    }

    impl Drop for TestNode {
        fn drop(&mut self) {
            self.server.abort();
        }
    }

    #[tokio::test]
    async fn availability_requires_success_and_explicit_readiness() {
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        for (status, body, expected) in [
            (StatusCode::OK, r#"{"isReady":true}"#, true),
            (StatusCode::OK, r#"{"isReady":false}"#, false),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                r#"{"isReady":true}"#,
                false,
            ),
            (StatusCode::OK, "{}", false),
            (StatusCode::OK, r#"{"isReady":"true"}"#, false),
            (StatusCode::OK, "invalid JSON", false),
        ] {
            let node = TestNode::start(status, body).await;
            assert_eq!(
                is_node_available(&client, &node.socket).await,
                expected,
                "status={status}, body={body}"
            );
        }
    }

    #[tokio::test]
    async fn unresponsive_node_is_unavailable() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let node = NodeSocket {
            host: "127.0.0.1".to_string(),
            grpc_port: 0,
            http_port: listener.local_addr().unwrap().port(),
        };
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();
        assert!(!is_node_available(&client, &node).await);
    }

    #[tokio::test]
    async fn selection_uses_only_a_ready_node() {
        let not_ready = TestNode::start(StatusCode::OK, r#"{"isReady":false}"#).await;
        let failed = TestNode::start(StatusCode::SERVICE_UNAVAILABLE, r#"{"isReady":true}"#).await;
        let ready = TestNode::start(StatusCode::OK, r#"{"isReady":true}"#).await;
        let nodes = [
            not_ready.socket.clone(),
            failed.socket.clone(),
            ready.socket.clone(),
        ];
        let selected = choose_random_node(&nodes).await.unwrap();
        assert_eq!(selected.http_port, ready.socket.http_port);
    }

    #[tokio::test]
    async fn selection_fails_when_no_node_is_ready() {
        let node = TestNode::start(StatusCode::OK, r#"{"isReady":false}"#).await;
        let error = choose_random_node(std::slice::from_ref(&node.socket))
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "No reachable nodes");
    }

    #[tokio::test]
    async fn selection_rejects_an_empty_inventory() {
        let error = choose_random_node(&[]).await.unwrap_err();
        assert_eq!(error.to_string(), "No available node sockets");
    }

    #[test]
    fn deploy_ids_enforce_length_and_reject_url_characters() {
        for length in [100, 130, 160] {
            assert!(validate_deploy_id(&"a1".repeat(length / 2)));
        }
        for length in [0, 99, 161] {
            assert!(!validate_deploy_id(&"a".repeat(length)));
        }
        for suffix in ["/", "?", "#", " ", "\n", "é"] {
            assert!(!validate_deploy_id(&format!("{}{suffix}", "a".repeat(120))));
        }
    }
}
