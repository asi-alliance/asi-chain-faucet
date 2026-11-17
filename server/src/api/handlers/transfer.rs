use crate::{
    api::handlers::balance_handler,
    api::models::{ApiResult, BalanceResponse, ErrorResponse, TransferRequest, TransferResponse},
    services::node_cli::{NodeCliService, TransferSubmissionError},
    AppState,
};
use axum::{extract::State, http::StatusCode, response::Json, Json as RequestJson};
use node_cli::vault::validate_address;
use tracing::{error, info, warn};

async fn ensure_recipient_balance_below_limit(
    state: &AppState,
    address: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let balance_json = balance_handler(
        State(state.clone()),
        axum::extract::Path(address.to_string()),
    )
    .await?;

    let Json(BalanceResponse { balance }) = balance_json;

    let eligible =
        crate::config::is_eligible(&balance, &state.config.faucet_max_balance).map_err(|_| {
            warn!(
                "FAUCET: Unable to parse balance '{}' for address {}",
                balance, address
            );
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::validation_error(
                    "FAUCET: Unable to parse existing balance for address",
                )),
            )
        })?;

    if !eligible {
        warn!(
            "FAUCET: Address {} balance {} exceeds faucet limit {}",
            address, balance, state.config.faucet_max_balance
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::validation_error(
                "Address balance exceeds faucet eligibility threshold",
            )),
        ));
    }

    Ok(())
}

#[axum::debug_handler]
pub async fn transfer_handler(
    State(state): State<AppState>,
    RequestJson(request): RequestJson<TransferRequest>,
) -> ApiResult<TransferResponse> {
    info!(
        "FAUCET: Transfer request received for address: {}",
        request.to_address
    );

    validate_address(&request.to_address).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::validation_error(&e.to_string())),
        )
    })?;

    ensure_recipient_balance_below_limit(&state, &request.to_address).await?;

    let private_key = state
        .config
        .private_key
        .as_ref()
        .expect("PRIVATE_KEY presence is validated at startup")
        .expose()
        .to_string();

    let node_cli_service = NodeCliService::new(state.config.clone(), state.alerts.clone());
    match node_cli_service
        .transfer_funds(&request.to_address, &private_key)
        .await
    {
        Ok(deploy_id) => {
            info!(
                "FAUCET: Transfer to {} deployed with id {}",
                &request.to_address, deploy_id
            );

            Ok(Json(TransferResponse {
                deploy_id: Some(deploy_id),
            }))
        }
        Err(e) => {
            error!(
                "FAUCET: Transfer failed to {} with error {}",
                request.to_address, e
            );
            Err(transfer_error_response(&e))
        }
    }
}

/// Map a submission failure to an HTTP answer that keeps the node's exact code
/// and the signed deploy ID:
///
/// - retryable admission refusals (`deploy_pool_busy`, `deploy_admission_busy`,
///   `invalid_deploy_window`): 503, a later request can succeed;
/// - final admission refusals of the faucet's own deploy (`deploy_too_large`,
///   `deploy_expired`): 502, the requester cannot fix them;
/// - untyped node rejections: 400 `deploy_rejected`;
/// - lost or malformed responses: 502 `deploy_submission_unknown`.
fn transfer_error_response(error: &anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    let (status, code, deploy_id) = match error.downcast_ref::<TransferSubmissionError>() {
        Some(failure @ TransferSubmissionError::Admission { reason, .. }) => (
            if reason.is_retryable() {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::BAD_GATEWAY
            },
            reason.code(),
            Some(failure.deploy_id().to_string()),
        ),
        Some(failure @ TransferSubmissionError::Unknown { .. }) => (
            StatusCode::BAD_GATEWAY,
            "deploy_submission_unknown",
            Some(failure.deploy_id().to_string()),
        ),
        Some(failure @ TransferSubmissionError::Rejected { .. }) => (
            StatusCode::BAD_REQUEST,
            "deploy_rejected",
            Some(failure.deploy_id().to_string()),
        ),
        None => (StatusCode::BAD_GATEWAY, "FAUCET: Transfer failed", None),
    };
    let mut body = ErrorResponse::new(code.to_string(), Some(error.to_string()));
    if let Some(id) = deploy_id {
        body = body.with_deploy_id(id);
    }
    (status, Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use node_cli::DeployAdmissionReason;

    fn admission(reason: DeployAdmissionReason) -> anyhow::Error {
        anyhow::Error::new(TransferSubmissionError::Admission {
            deploy_id: "original-id".into(),
            reason,
            message: format!("{}: not accepted", reason.code()),
        })
    }

    #[test]
    fn admission_refusals_retain_the_exact_code_and_id() {
        for (reason, status) in [
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
        ] {
            let (actual, Json(body)) = transfer_error_response(&admission(reason));
            assert_eq!(actual, status, "{reason}");
            assert_eq!(body.error, reason.code());
            assert_eq!(body.deploy_id.as_deref(), Some("original-id"));
            assert!(body.details.unwrap().contains("not accepted"));
        }
    }

    #[test]
    fn unknown_results_are_never_presented_as_admission_refusals() {
        let error = anyhow::Error::new(TransferSubmissionError::Unknown {
            deploy_id: "original-id".into(),
            cause: node_cli::Error::Decode {
                what: "deploy response",
                detail: "missing".into(),
            },
        });
        let (status, Json(body)) = transfer_error_response(&error);
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body.error, "deploy_submission_unknown");
        assert_eq!(body.deploy_id.as_deref(), Some("original-id"));
        assert!(body.details.unwrap().contains("outcome unknown"));
    }
}
