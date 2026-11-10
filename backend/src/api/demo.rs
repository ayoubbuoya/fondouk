//! Demo endpoints: control simulation speed, trigger market shocks, and force agent ticks
//! for live demo recording.

use std::str::FromStr;

use axum::{Json, extract::State};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{ApiError, AppState};
use crate::agent::tick::{TickOptions, tick_once_demo};
use crate::db::villages;
use crate::db::yield_snapshots::{self, YieldSnapshot};
use crate::indexer;

/// Request payload for `POST /demo/shock`.
#[derive(Debug, Deserialize, Default)]
pub struct ShockRequest {
    pub village_id: Option<String>,
    pub apy: Option<Decimal>,
    pub preset: Option<String>,
}

/// Response payload for `POST /demo/shock`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ShockResponse {
    pub status: String,
    pub village_id: String,
    pub apy: Decimal,
}

/// Request payload for `POST /demo/tick`.
#[derive(Debug, Deserialize, Default)]
pub struct TickRequest {
    pub relax_arena_rate_limit: Option<bool>,
    pub bypass_cooldown: Option<bool>,
}

/// Response payload for `POST /demo/tick`.
#[derive(Debug, Serialize, Deserialize)]
pub struct TickResponse {
    pub status: String,
    pub decisions: usize,
}

/// Response payload for `POST /demo/step`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StepResponse {
    pub status: String,
    pub shock: ShockResponse,
    pub tick: TickResponse,
}

/// Response payload for `POST /demo/reset`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResetResponse {
    pub status: String,
    pub message: String,
}

/// Resolves a market shock request into a target village ID and target APY.
fn resolve_shock(req: &ShockRequest) -> (String, Decimal) {
    if let (Some(village_id), Some(apy)) = (&req.village_id, req.apy) {
        return (village_id.clone(), apy);
    }

    match req.preset.as_deref() {
        Some("dex_spike") => (
            "rhea-dex-frax-usdc".to_string(),
            Decimal::from_str("25.0").unwrap_or(Decimal::from(25)),
        ),
        Some("usdc_drop") => (
            "rhea-lend-usdc".to_string(),
            Decimal::from_str("3.2").unwrap_or(Decimal::from(3)),
        ),
        // Default preset: "usdt_spike"
        _ => (
            "rhea-lend-usdt".to_string(),
            Decimal::from_str("22.5").unwrap_or(Decimal::from(22)),
        ),
    }
}

/// Handler for `POST /demo/shock`: injects a sudden yield shift and broadcasts `apr_update`.
pub async fn market_shock(
    State(state): State<AppState>,
    Json(req): Json<ShockRequest>,
) -> Result<Json<ShockResponse>, ApiError> {
    let (village_id, apy) = resolve_shock(&req);
    let now = Utc::now();

    let snapshot = YieldSnapshot {
        village_id: village_id.clone(),
        ts: now,
        apy_base: apy,
        apy_reward: Decimal::ZERO,
        tvl_usd: Decimal::from(1_500_000),
    };

    yield_snapshots::insert_snapshot(&state.db, &snapshot).await?;
    let _ = villages::mark_fetched(&state.db, &village_id, now).await;
    indexer::announce_villages(&state.db, state.village_stale_after, &state.events, now).await;

    Ok(Json(ShockResponse {
        status: "ok".to_string(),
        village_id,
        apy,
    }))
}

/// Handler for `POST /demo/tick`: immediately runs one tick across all active sim avatars.
pub async fn trigger_tick(
    State(state): State<AppState>,
    Json(req): Json<TickRequest>,
) -> Result<Json<TickResponse>, ApiError> {
    let llm = state.llm.as_ref().ok_or_else(|| {
        ApiError::BadRequest(
            "LLM is not configured (set LLM_BASE_URL, LLM_API_KEY, LLM_MODEL in backend/.env)".into(),
        )
    })?;

    let options = TickOptions {
        relax_arena_rate_limit: req.relax_arena_rate_limit.unwrap_or(true),
        bypass_cooldown: req.bypass_cooldown.unwrap_or(true),
    };
    let now = Utc::now();

    let decisions = tick_once_demo(
        &state.db,
        llm,
        state.village_stale_after,
        &state.events,
        now,
        options,
    )
    .await?;

    Ok(Json(TickResponse {
        status: "ok".to_string(),
        decisions,
    }))
}

/// Handler for `POST /demo/step`: combines a yield shock and an immediate agent tick.
pub async fn demo_step(
    State(state): State<AppState>,
    Json(req): Json<ShockRequest>,
) -> Result<Json<StepResponse>, ApiError> {
    let shock_res = market_shock(State(state.clone()), Json(req)).await?;
    let tick_res = trigger_tick(
        State(state),
        Json(TickRequest {
            relax_arena_rate_limit: Some(true),
            bypass_cooldown: Some(true),
        }),
    )
    .await?;

    Ok(Json(StepResponse {
        status: "ok".to_string(),
        shock: shock_res.0,
        tick: tick_res.0,
    }))
}

/// Handler for `POST /demo/reset`: resets village yields back to standard DefiLlama baselines.
pub async fn reset_rates(
    State(state): State<AppState>,
) -> Result<Json<ResetResponse>, ApiError> {
    let now = Utc::now();
    let baselines = [
        ("rhea-lend-usdc", Decimal::from_str("14.0").unwrap_or(Decimal::from(14))),
        ("rhea-lend-usdt", Decimal::from_str("13.1").unwrap_or(Decimal::from(13))),
        ("rhea-dex-stable4", Decimal::from_str("8.1").unwrap_or(Decimal::from(8))),
        ("rhea-dex-frax-usdc", Decimal::from_str("11.3").unwrap_or(Decimal::from(11))),
    ];

    for (village_id, apy) in baselines {
        let snapshot = YieldSnapshot {
            village_id: village_id.to_string(),
            ts: now,
            apy_base: apy,
            apy_reward: Decimal::ZERO,
            tvl_usd: Decimal::from(1_500_000),
        };
        let _ = yield_snapshots::insert_snapshot(&state.db, &snapshot).await;
        let _ = villages::mark_fetched(&state.db, village_id, now).await;
    }

    indexer::announce_villages(&state.db, state.village_stale_after, &state.events, now).await;

    Ok(Json(ResetResponse {
        status: "ok".to_string(),
        message: "reset to baseline APYs".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use serde_json::json;
    use sqlx::PgPool;

    use crate::api::router;
    use crate::api::tests::{get, request, send, test_state};

    #[sqlx::test]
    async fn test_demo_shock_and_reset(pool: PgPool) {
        let state = test_state(pool);

        // 1. Trigger USDT spike
        let req = request(
            "POST",
            "/demo/shock",
            None,
            Some(json!({ "preset": "usdt_spike" })),
        );
        let (status, _, body) = send(router(state.clone()), req).await;
        assert_eq!(status, StatusCode::OK);
        let shock_res: ShockResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(shock_res.status, "ok");
        assert_eq!(shock_res.village_id, "rhea-lend-usdt");
        assert_eq!(shock_res.apy, Decimal::from_str("22.5").unwrap());

        // Verify GET /villages reflects the spiked APY
        let (status, body) = get(router(state.clone()), "/villages").await;
        assert_eq!(status, StatusCode::OK);
        let villages: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        let usdt = villages.iter().find(|v| v["id"] == "rhea-lend-usdt").unwrap();
        assert_eq!(usdt["apy_base"].as_str().unwrap(), "22.5");

        // 2. Trigger reset
        let req = request("POST", "/demo/reset", None, Some(json!({})));
        let (status, _, body) = send(router(state.clone()), req).await;
        assert_eq!(status, StatusCode::OK);
        let reset_res: ResetResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(reset_res.status, "ok");

        // Verify GET /villages reflects the reset APY (13.1%)
        let (status, body) = get(router(state), "/villages").await;
        assert_eq!(status, StatusCode::OK);
        let villages: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        let usdt = villages.iter().find(|v| v["id"] == "rhea-lend-usdt").unwrap();
        assert_eq!(usdt["apy_base"].as_str().unwrap(), "13.1");
    }

    #[sqlx::test]
    async fn test_demo_tick_without_llm_returns_bad_request(pool: PgPool) {
        let state = test_state(pool); // has llm: None
        let req = request("POST", "/demo/tick", None, Some(json!({})));
        let (status, _, body) = send(router(state), req).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(val["error"].as_str().unwrap().contains("LLM is not configured"));
    }
}
