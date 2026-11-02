//! Fondouk backend: one binary that will host the indexer, agent loop and API.

mod api;
mod config;

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // A missing .env is fine (e.g. in production, where real env vars are set).
    dotenvy::dotenv().ok();

    // Log level comes from RUST_LOG (e.g. "info" or "backend=debug"); default "info".
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env()?;

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("could not listen on {}", config.bind_addr))?;
    tracing::info!("listening on {}", config.bind_addr);

    axum::serve(listener, api::router())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTP server stopped with an error")?;

    tracing::info!("shut down cleanly");
    Ok(())
}

/// Resolves when the user presses Ctrl+C, so in-flight requests can finish
/// before the process exits (later this also lets the indexer stop cleanly).
async fn shutdown_signal() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        tracing::error!("could not listen for Ctrl+C: {err}");
    }
}
