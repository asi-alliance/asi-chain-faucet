use crate::{
    api::models::{ApiResult, ErrorResponse, TransferRequest, TransferResponse},
    services::{
        claims::ClaimError,
        node_cli::{temporary_admission_refusal, NodeCliService},
    },
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
        .map_err(claim_error_response)?;
    Ok(Json(TransferResponse {
        deploy_id: Some(deploy_id),
    }))
}

fn claim_error_response(error: ClaimError) -> (StatusCode, Json<ErrorResponse>) {
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
        ClaimError::Submission { deploy_id, error } => {
            let (status, code, details) = match error.downcast_ref::<node_cli::Error>() {
                Some(node_cli::Error::DeployAdmissionRefused {
                    reason, message, ..
                }) => (
                    if temporary_admission_refusal(*reason) {
                        StatusCode::SERVICE_UNAVAILABLE
                    } else {
                        StatusCode::BAD_GATEWAY
                    },
                    reason.code(),
                    message.clone(),
                ),
                Some(node_cli::Error::NodeRejected { message, .. })
                    if !message.trim().is_empty() =>
                {
                    (StatusCode::BAD_REQUEST, "deploy_rejected", message.clone())
                }
                _ => (
                    StatusCode::BAD_GATEWAY,
                    "deploy_submission_unknown",
                    "Submission could not be confirmed. The saved claim remains reserved.".into(),
                ),
            };
            return (
                status,
                Json(ErrorResponse::new(code.into(), Some(details)).with_deploy_id(deploy_id)),
            );
        }
    };
    (
        status,
        Json(ErrorResponse::new(message.into(), Some(message.into()))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use node_cli::DeployAdmissionReason;

    #[test]
    fn refusals_report_the_reason_and_original_id() {
        for (reason, expected) in [
            (
                DeployAdmissionReason::PoolBusy,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (
                DeployAdmissionReason::AdmissionBusy,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (
                DeployAdmissionReason::OutsideWindow,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (DeployAdmissionReason::TooLarge, StatusCode::BAD_GATEWAY),
            (DeployAdmissionReason::Expired, StatusCode::BAD_GATEWAY),
            (
                DeployAdmissionReason::QueueFull,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (
                DeployAdmissionReason::DeployerShareFull,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (
                DeployAdmissionReason::QueueNodeUnavailable,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (
                DeployAdmissionReason::QueueStopped,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        ] {
            let (actual, Json(body)) = claim_error_response(ClaimError::Submission {
                deploy_id: "original-id".into(),
                error: node_cli::Error::DeployAdmissionRefused {
                    deploy_id: "original-id".into(),
                    reason,
                    message: "Declined".into(),
                    retry_after: None,
                    congestion: None,
                }
                .into(),
            });
            assert_eq!(actual, expected, "{reason}");
            assert_eq!(body.error, reason.code());
            assert_eq!(body.deploy_id.as_deref(), Some("original-id"));
        }
    }

    #[test]
    fn unknown_submission_returns_the_original_id() {
        let (status, Json(body)) = claim_error_response(ClaimError::Submission {
            deploy_id: "original-id".into(),
            error: anyhow::anyhow!("Transport error"),
        });
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body.error, "deploy_submission_unknown");
        assert_eq!(body.deploy_id.as_deref(), Some("original-id"));
    }
}
