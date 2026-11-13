mod api;
mod config;
mod core;
mod services;
mod utils;

use anyhow::{Context, Result};
use tracing::{debug, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub use config::AppConfig;
pub use core::{AppState, Application};

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    info!("Starting ASI Faucet service");

    let config = AppConfig::from_env()?;
    info!("Configuration loaded");
    info!(
        token_decimals = config.native_token_decimals,
        faucet_amount_asi = %node_cli::vault::dust_to_tokens(&config.faucet_amount, config.native_token_decimals)?,
        faucet_amount_atomic = %config.faucet_amount,
        faucet_max_balance_asi = %node_cli::vault::dust_to_tokens(&config.faucet_max_balance, config.native_token_decimals)?,
        faucet_max_balance_atomic = %config.faucet_max_balance,
        "Effective faucet amount and eligibility ceiling"
    );

    debug!("Server config = {:?}", config);

    let app = Application::build(config)
        .await
        .context("Failed to build application")?;

    info!("Application built successfully");

    app.run().await.context("Application runtime error")?;

    info!("Application shutting down gracefully");
    Ok(())
}

fn setup_logging() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "asi_faucet=info,tower_http=debug,axum=debug".into());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();
}
