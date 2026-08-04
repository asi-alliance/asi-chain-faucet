use crate::{
    api::models::{ApiResult, DeployStatusResponse, ErrorResponse},
    services::node_cli::NodeCliService,
    utils::validate_deploy_id,
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use node_cli::f1r3fly_api::DeployFinalizationStatus;
use tracing::{error, info};

fn to_status_response(info: &DeployFinalizationStatus) -> DeployStatusResponse {
    let (status, msg) = match info.state.as_str() {
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
    let node_cli_service = NodeCliService::new(state.config.clone());

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
