//! Fondouk backend: one binary that will host the indexer, agent loop and API.
//! The logic lives in the library (`lib.rs`); this file only wires it together.

use std::time::Duration;

use anyhow::{Context, Result};
use backend::{agent, api, config::Config, db, indexer, live_events};
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

    // Live events for /ws: the indexer and the agent loop publish, each
    // browser connection subscribes (D43).
    let events = live_events::channel();

    let indexer_task = tokio::spawn(indexer::run(
        db_pool.clone(),
        http.clone(),
        config.indexer_interval,
        config.village_stale_after,
        events.clone(),
        shutdown_rx.clone(),
    ));

    // One LLM client, shared by the agent loop and the create-time backtest
    // (D63). Without LLM_* settings the rest still runs.
    let llm = config.llm.as_ref().map(agent::llm::LlmClient::new);
    let agent_task = match &llm {
        Some(llm) => Some(tokio::spawn(agent::tick::run(
            db_pool.clone(),
            llm.clone(),
            config.agent_tick_interval,
            config.village_stale_after,
            events.clone(),
            shutdown_rx.clone(),
        ))),
        None => {
            tracing::warn!("LLM_* settings not set: agent loop is off (see backend/.env.example)");
            None
        }
    };

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("could not listen on {}", config.bind_addr))?;
    tracing::info!("listening on {}", config.bind_addr);

    let mut server_shutdown = shutdown_rx;
    let app_state = api::AppState {
        db: db_pool,
        village_stale_after: config.village_stale_after,
        events,
        http,
        near_rpc_url: config.near_rpc_url.clone(),
        marketplace_contract_id: config.marketplace_contract_id.clone(),
        vault_contract_id: config.vault_contract_id.clone(),
        test_token_contract_id: config.test_token_contract_id.clone(),
        llm,
    };
    axum::serve(listener, api::router(app_state))
        .with_graceful_shutdown(async move { indexer::stop_requested(&mut server_shutdown).await })
        .await
        .context("HTTP server stopped with an error")?;

    // Let the indexer finish its current round before the process exits.
    if let Err(err) = indexer_task.await {
        tracing::error!("indexer task ended abnormally: {err}");
    }
    // Same for the agent loop: a tick in progress finishes its writes.
    if let Some(task) = agent_task
        && let Err(err) = task.await
    {
        tracing::error!("agent task ended abnormally: {err}");
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
