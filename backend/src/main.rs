//! Fondouk backend: one binary that will host the indexer, agent loop and API.
//! The logic lives in the library (`lib.rs`); this file only wires it together.

use anyhow::{Context, Result};
use backend::{api, config::Config, db};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // A missing .env is fine (e.g. in production, where real env vars are set).
    dotenvy::dotenv().ok();

    // Log level comes from RUST_LOG (e.g. "info" or "backend=debug"); default "info".
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env()?;

    // Connect now so a stopped Postgres or wrong credentials fail at startup.
    // The pool is handed to the indexer in Phase 3; until then it is only held open.
    let _db_pool = db::connect(&config.database_url)
        .await
        .context("could not connect to the database (is Postgres running? is DATABASE_URL right?)")?;
    tracing::info!("connected to the database");

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
