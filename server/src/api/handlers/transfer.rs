use crate::{
    api::models::{ApiResult, ErrorResponse, TransferRequest, TransferResponse},
    services::{
        claims::ClaimError,
        node_cli::{temporary_admission_refusal, NodeCliService, NotSubmitted, QueueCause},
    },
    AppState,
};
use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    Json as RequestJson,
};
use node_cli::vault::validate_address;
use std::time::Duration;

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
            .into_response()
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
        .map_err(IntoResponse::into_response)?;
    Ok(Json(TransferResponse {
        deploy_id: Some(deploy_id),
    }))
}

impl IntoResponse for ClaimError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
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
            ClaimError::WaitingForFreshDeploy {
                deploy_id,
                cause,
                retry_after,
            } => {
                return SubmissionError {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    code: "deploy_queue_full",
                    details:
                        "The queue refused the previous deploy. Retry after the indicated wait."
                            .into(),
                    cause: Some(cause),
                    retry_after: Some(retry_after),
                    deploy_id,
                }
                .into_response();
            }
            ClaimError::Submission {
                deploy_id,
                error,
                prior_acceptance_possible,
            } => {
                return SubmissionError::classify(deploy_id, &error, prior_acceptance_possible)
                    .into_response()
            }
        };
        (
            status,
            Json(ErrorResponse::new(message.into(), Some(message.into()))),
        )
            .into_response()
    }
}

/// A submission failure as the client sees it. Every variant returns the
/// journaled `deploy_id`, because the reservation survives the failure.
struct SubmissionError {
    status: StatusCode,
    code: &'static str,
    cause: Option<QueueCause>,
    details: String,
    deploy_id: String,
    retry_after: Option<Duration>,
}

impl SubmissionError {
    fn classify(deploy_id: String, error: &anyhow::Error, prior_acceptance_possible: bool) -> Self {
        let (status, code, details, retry_after) = match error.downcast_ref::<node_cli::Error>() {
            Some(node_cli::Error::DeployAdmissionRefused {
                reason,
                message,
                retry_after,
                ..
            }) => (
                if temporary_admission_refusal(*reason) {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::BAD_GATEWAY
                },
                reason.code(),
                message.clone(),
                *retry_after,
            ),
            Some(node_cli::Error::NodeRejected { message, .. }) if !message.trim().is_empty() => (
                // The faucet's own deploy was refused, not the client's request.
                StatusCode::BAD_GATEWAY,
                "deploy_rejected",
                message.clone(),
                None,
            ),
            // A pre-send failure is conclusive if every earlier attempt was also refused or unsent.
            _ if !prior_acceptance_possible && error.downcast_ref::<NotSubmitted>().is_some() => (
                StatusCode::SERVICE_UNAVAILABLE,
                "deploy_not_submitted",
                "No validator passed verification for this attempt. The saved claim remains reserved.".into(),
                None,
            ),
            _ => (
                StatusCode::BAD_GATEWAY,
                "deploy_submission_unknown",
                "Submission could not be confirmed. The saved claim remains reserved.".into(),
                None,
            ),
        };
        Self {
            status,
            code,
            cause: error
                .downcast_ref::<node_cli::Error>()
                .and_then(QueueCause::from_refusal),
            details,
            deploy_id,
            retry_after,
        }
    }

    fn into_response(self) -> Response {
        let mut body =
            ErrorResponse::new(self.code.into(), Some(self.details)).with_deploy_id(self.deploy_id);
        body.cause = self.cause;
        let mut response = (self.status, Json(body)).into_response();
        if let Some(retry_after) = self.retry_after {
            if let Ok(seconds) = HeaderValue::from_str(&retry_after.as_secs().to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, seconds);
            }
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use node_cli::DeployAdmissionReason;

    /// Render a claim failure the way the handler does: status, headers, body.
    async fn render(error: ClaimError) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, headers, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn refusals_report_the_reason_and_original_id() {
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
            let (status, headers, body) = render(ClaimError::Submission {
                prior_acceptance_possible: false,
                deploy_id: "original-id".into(),
                error: node_cli::Error::DeployAdmissionRefused {
                    deploy_id: "original-id".into(),
                    reason,
                    message: "Declined".into(),
                    retry_after: None,
                    congestion: None,
                }
                .into(),
            })
            .await;
            assert_eq!(status, expected, "{reason}");
            assert_eq!(body["error"], reason.code());
            assert_eq!(body["deploy_id"], "original-id");
            assert!(headers.get(header::RETRY_AFTER).is_none(), "{reason}");
        }
    }

    #[tokio::test]
    async fn queue_causes_are_optional_and_machine_readable() {
        use node_cli::QueueCongestion::*;
        use DeployAdmissionReason::{DeployerShareFull, QueueFull};
        for (reason, congestion, expected) in [
            (QueueFull, Some(Validator), Some("validator")),
            (QueueFull, Some(ValidatorIdle), Some("validator_idle")),
            (QueueFull, Some(Shard), Some("shard")),
            (QueueFull, Some(Storage), Some("storage")),
            (DeployerShareFull, None, Some("deployer_share")),
            (QueueFull, None, None),
        ] {
            let (_, _, body) = render(ClaimError::Submission {
                prior_acceptance_possible: false,
                deploy_id: "id".into(),
                error: node_cli::Error::DeployAdmissionRefused {
                    deploy_id: "id".into(),
                    reason,
                    congestion,
                    message: "full".into(),
                    retry_after: None,
                }
                .into(),
            })
            .await;
            assert_eq!(
                body.get("cause").map(|cause| cause.as_str().unwrap()),
                expected
            );
        }
    }

    #[tokio::test]
    async fn a_queue_retry_hint_becomes_a_retry_after_header() {
        let (status, headers, body) = render(ClaimError::Submission {
            prior_acceptance_possible: false,
            deploy_id: "original-id".into(),
            error: node_cli::Error::DeployAdmissionRefused {
                deploy_id: "original-id".into(),
                reason: DeployAdmissionReason::QueueFull,
                message: "Queue is full".into(),
                retry_after: Some(Duration::from_secs(7)),
                congestion: None,
            }
            .into(),
        })
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"], "deploy_queue_full");
        assert_eq!(headers[header::RETRY_AFTER], "7");
    }

    #[tokio::test]
    async fn unknown_submission_returns_the_original_id() {
        let (status, _, body) = render(ClaimError::Submission {
            prior_acceptance_possible: false,
            deploy_id: "original-id".into(),
            error: anyhow::anyhow!("Transport error"),
        })
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body["error"], "deploy_submission_unknown");
        assert_eq!(body["deploy_id"], "original-id");
    }

    #[tokio::test]
    async fn an_untyped_rejection_is_an_upstream_failure() {
        let (status, _, body) = render(ClaimError::Submission {
            prior_acceptance_possible: false,
            deploy_id: "original-id".into(),
            error: node_cli::Error::NodeRejected {
                operation: "doDeploy",
                message: "Phlo price below node minimum".into(),
            }
            .into(),
        })
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body["error"], "deploy_rejected");
        assert_eq!(body["deploy_id"], "original-id");
    }

    #[tokio::test]
    async fn a_deploy_that_never_left_the_faucet_is_not_an_unknown_outcome() {
        let (status, _, body) = render(ClaimError::Submission {
            prior_acceptance_possible: false,
            deploy_id: "original-id".into(),
            error: anyhow::Error::new(NotSubmitted::new(anyhow::anyhow!(
                "Validator has incompatible network, shard, API version or token decimals"
            ))),
        })
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"], "deploy_not_submitted");
        assert_eq!(body["deploy_id"], "original-id");
    }
}
