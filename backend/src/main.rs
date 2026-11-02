//! Fondouk backend: one binary that will host the indexer, agent loop and API.
//! The logic lives in the library (`lib.rs`); this file only wires it together.

use std::time::Duration;

use anyhow::{Context, Result};
use backend::{api, config::Config, db, indexer};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

/// Upper bound for one HTTP request to an external API (DefiLlama today).
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

#[tokio::main]
async fn main() -> Result<()> {
    // A missing .env is fine (e.g. in production, where real env vars are set).
    dotenvy::dotenv().ok();

    // Log level comes from RUST_LOG (e.g. "info" or "backend=debug"); default "info".
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env()?;
    tracing::info!(?config, "starting"); // Debug output hides the database password

    // Connect now so a stopped Postgres or wrong credentials fail at startup.
    let db_pool = db::connect(&config.database_url).await.context(
        "could not connect to the database (is Postgres running? is DATABASE_URL right?)",
    )?;
    tracing::info!("connected to the database");

    // One shutdown switch shared by every long-running part. Ctrl+C flips it to
    // `true`; the HTTP server and the indexer each stop when they see it.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        wait_for_ctrl_c().await;
        tracing::info!("shutdown requested");
        let _ = shutdown_tx.send(true);
    });

    let http = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(concat!("fondouk-backend/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("could not build the HTTP client")?;
    let indexer_task = tokio::spawn(indexer::run(
        db_pool.clone(),
        http,
        config.indexer_interval,
        shutdown_rx.clone(),
    ));

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("could not listen on {}", config.bind_addr))?;
    tracing::info!("listening on {}", config.bind_addr);

    let mut server_shutdown = shutdown_rx;
    axum::serve(listener, api::router())
        .with_graceful_shutdown(async move { indexer::stop_requested(&mut server_shutdown).await })
        .await
        .context("HTTP server stopped with an error")?;

    // Let the indexer finish its current round before the process exits.
    if let Err(err) = indexer_task.await {
        tracing::error!("indexer task ended abnormally: {err}");
    }

    tracing::info!("shut down cleanly");
    Ok(())
}

/// Resolves when the user presses Ctrl+C.
async fn wait_for_ctrl_c() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        tracing::error!("could not listen for Ctrl+C: {err}");
    }
}
