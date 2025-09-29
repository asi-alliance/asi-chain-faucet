use crate::{
    api::models::{ApiResult, BalanceResponse, ErrorResponse},
    services::node_cli::NodeCliService,
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use node_cli::commands::validate_rev_address;
use tracing::{error, info};

pub async fn balance_handler(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> ApiResult<BalanceResponse> {
    info!("FAUCET: Balance request received for address: {}", address);

    validate_rev_address(&address).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::validation_error(e.to_string().as_str())),
        )
    })?;

    let node_cli_service = NodeCliService::new(state.config.clone());
    match node_cli_service.get_balance(&address).await {
        Ok(balance) => {
            info!(
                "FAUCET: Balance retrieval successful for {}: {}",
                address, balance
            );
            Ok(Json(BalanceResponse { balance: balance }))
        }
        Err(e) => {
            error!("FAUCET: Balance retrieval failed: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "FAUCET: Balance retrieval failed".to_string(),
                    Some(e.to_string()),
                )),
            ))
        }
    }
}
