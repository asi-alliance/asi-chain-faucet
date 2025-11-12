use crate::{
    api::handlers::balance_handler,
    api::models::{ApiResult, BalanceResponse, ErrorResponse, TransferRequest, TransferResponse},
    services::node_cli::NodeCliService,
    AppState,
};
use axum::{extract::State, http::StatusCode, response::Json, Json as RequestJson};
use node_cli::commands::validate_address;
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

    let balance_value: u128 = balance.parse().map_err(|_| {
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

    let max_balance_allowed: u128 = state.config.faucet_max_balance as u128 * 10u128.pow(8);
    if balance_value >= max_balance_allowed {
        warn!(
            "FAUCET: Address {} balance {} exceeds faucet limit {}",
            address, balance_value, max_balance_allowed
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
            Json(ErrorResponse::validation_error(e.to_string().as_str())),
        )
    })?;

    ensure_recipient_balance_below_limit(&state, &request.to_address).await?;

    let private_key = state.config.private_key.clone().unwrap();

    let node_cli_service = NodeCliService::new(state.config.clone());
    match node_cli_service
        .transfer_funds(&request.to_address, private_key)
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
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "FAUCET: Transfer failed".to_string(),
                    Some(e.to_string()),
                )),
            ))
        }
    }
}
