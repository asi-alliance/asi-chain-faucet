use crate::{
    api::models::{ApiResult, ErrorResponse, TransferRequest, TransferResponse},
    services::{claims::ClaimError, node_cli::NodeCliService},
    AppState,
};
use axum::{extract::State, http::StatusCode, response::Json, Json as RequestJson};
use node_cli::vault::validate_address;

#[axum::debug_handler]
pub async fn transfer_handler(
    State(state): State<AppState>,
    RequestJson(request): RequestJson<TransferRequest>,
) -> ApiResult<TransferResponse> {
    validate_address(&request.to_address).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::validation_error(&error.to_string())),
        )
    })?;
    let node = NodeCliService::new(state.config.clone(), state.alerts.clone());
    let deploy_id = state
        .claims
        .claim(
            &node,
            &request.to_address,
            &state.config.faucet_amount,
            &state.config.faucet_max_balance,
        )
        .await
        .map_err(|error| {
            let (status, message) = match error {
                ClaimError::Busy => (
                    StatusCode::CONFLICT,
                    "A claim for this address is being processed. Retry shortly.",
                ),
                ClaimError::Ineligible => (
                    StatusCode::BAD_REQUEST,
                    "Address balance must be strictly below the pre-claim eligibility threshold.",
                ),
                ClaimError::Unavailable(error) => {
                    tracing::error!(%error, "Claim could not be reconciled or persisted");
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Claim unavailable. Any existing reservation is retained; retry later.",
                    )
                }
            };
            (
                status,
                Json(ErrorResponse::new(message.into(), Some(message.into()))),
            )
        })?;
    Ok(Json(TransferResponse {
        deploy_id: Some(deploy_id),
    }))
}
