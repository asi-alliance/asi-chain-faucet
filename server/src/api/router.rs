use crate::{
    api::handlers::{balance_handler, deploy_info_handler, transfer_handler},
    api::middleware::request_id::RequestIdLayer,
    AppState,
};
use axum::http::{
    header::{AUTHORIZATION, CONTENT_TYPE},
    Method, StatusCode,
};
use axum::{
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use std::time::Duration;
use tower_http::cors::Any;
use tower_http::{
    compression::CompressionLayer, cors::CorsLayer, limit::RequestBodyLimitLayer,
    timeout::TimeoutLayer,
};

async fn preflight() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

async fn monetary_config(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "eligibility_policy": "strict_pre_claim_threshold",
        "native_token_decimals": state.config.native_token_decimals,
        "faucet_amount": state.config.faucet_amount.to_string(),
        "eligibility_ceiling": state.config.faucet_max_balance.to_string(),
    }))
}

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION])
        .max_age(Duration::from_secs(60 * 60));

    let api_routes = Router::new()
        .route("/config", get(monetary_config).options(preflight))
        .route("/transfer", post(transfer_handler).options(preflight))
        .route(
            "/deploy/{deploy_id}",
            get(deploy_info_handler)
                .options(preflight)
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(7),
                )),
        )
        .route(
            "/balance/{address}",
            get(balance_handler).options(preflight),
        );

    // Let exploratory retries and submission finish within the node client's
    // bounded requests instead of cancelling them at the old seven-second limit.
    api_routes
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .layer(RequestIdLayer::new())
        .layer(CompressionLayer::new())
        .with_state(state)
}
