use crate::{
    api::models::{ApiResult, DeployStatusResponse, ErrorResponse},
    services::node_cli::{DeployInfo, NodeCliService},
    utils::validate_deploy_id,
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use tracing::{error, info};

fn to_status_response(info: &DeployInfo) -> DeployStatusResponse {
    if let Some(err) = &info.system_deploy_error {
        return DeployStatusResponse {
            status: "DeployError".to_string(),
            msg: Some(err.clone()),
        };
    }

    if info.errored {
        return DeployStatusResponse {
            status: "DeployError".to_string(),
            msg: Some("Deploy execution errored".to_string()),
        };
    }

    let (status, msg) = match info.finalization.state.as_str() {
        "Finalized" => ("Finalized", None),
        "Failed" => ("DeployError", Some("Deploy execution failed".to_string())),
        "Expired" => ("DeployError", Some("Deploy expired".to_string())),
        "Pending" => ("Deploying", None),
        other => ("Unknown", Some(format!("Unknown deploy state: {other}"))),
    };

    DeployStatusResponse {
        status: status.to_string(),
        msg,
    }
}

#[axum::debug_handler]
pub async fn deploy_info_handler(
    State(state): State<AppState>,
    Path(deploy_id): Path<String>,
) -> ApiResult<DeployStatusResponse> {
    let node_cli_service = NodeCliService::new(state.config.clone(), state.alerts.clone());

    if !validate_deploy_id(&deploy_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::validation_error(
                "FAUCET: Invalid deploy ID format (must be 100-160 alphanumeric chars)",
            )),
        ));
    }

    match node_cli_service.get_deploy_info(deploy_id.clone()).await {
        Ok(deploy_info) => {
            info!(
                "FAUCET: Deploy info retrieved successfully for ID: {}",
                deploy_id
            );
            Ok(Json(to_status_response(&deploy_info)))
        }
        Err(e) => {
            error!("FAUCET: Failed to retrieve deploy info: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal_error(
                    "FAUCET: Failed to retrieve deploy info",
                )),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use node_cli::f1r3fly_api::DeployFinalizationStatus;
    use serde_json::json;

    fn deploy_info(state: &str) -> DeployInfo {
        DeployInfo {
            finalization: DeployFinalizationStatus {
                state: state.to_string(),
                rejection_count: 0,
                latest_block_hash: None,
            },
            system_deploy_error: None,
            errored: false,
        }
    }

    #[test]
    fn pending_and_finalized_responses_omit_error_messages() {
        for (state, expected) in [("Pending", "Deploying"), ("Finalized", "Finalized")] {
            let response = to_status_response(&deploy_info(state));
            assert_eq!(
                serde_json::to_value(response).unwrap(),
                json!({"status": expected})
            );
        }
    }

    #[test]
    fn failed_and_expired_deploys_expose_failure_reasons() {
        for (state, reason) in [
            ("Failed", "Deploy execution failed"),
            ("Expired", "Deploy expired"),
        ] {
            let response = to_status_response(&deploy_info(state));
            assert_eq!(
                serde_json::to_value(response).unwrap(),
                json!({"status": "DeployError", "msg": reason})
            );
        }
    }

    #[test]
    fn system_payment_error_overrides_finalized_success_and_generic_errors() {
        for errored in [false, true] {
            let mut info = deploy_info("Finalized");
            info.errored = errored;
            info.system_deploy_error =
                Some("Deploy payment failed: Insufficient funds".to_string());
            assert_eq!(
                serde_json::to_value(to_status_response(&info)).unwrap(),
                json!({
                    "status": "DeployError",
                    "msg": "Deploy payment failed: Insufficient funds"
                })
            );
        }
    }

    #[test]
    fn execution_error_cannot_be_reported_as_finalized_success() {
        let mut info = deploy_info("Finalized");
        info.errored = true;
        assert_eq!(
            serde_json::to_value(to_status_response(&info)).unwrap(),
            json!({"status": "DeployError", "msg": "Deploy execution errored"})
        );
    }

    #[test]
    fn unrecognized_node_state_is_not_reported_as_success() {
        assert_eq!(
            serde_json::to_value(to_status_response(&deploy_info("Unexpected"))).unwrap(),
            json!({"status": "Unknown", "msg": "Unknown deploy state: Unexpected"})
        );
    }
}
