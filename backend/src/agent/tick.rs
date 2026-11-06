//! The live tick loop (TECH §5.4): every `AGENT_TICK_SECS`, each active `sim`
//! avatar gets one decision, stored in `decisions` with `source = 'sim'`.
//!
//! Day 3 scope (HANDOFF D30): decisions are **logged only**. Positions are not
//! stored yet (the `positions` table and the live ledger come on Day 4), so
//! every tick sees the avatar's `cash_usd` and no positions.
//!
//! Avatars are handled one after the other. With 5 to 10 demo avatars and a
//! 10-minute tick that is plenty; parallel calls can come later if needed.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use super::decide::decide;
use super::guardrails::Risk;
use super::llm::LlmClient;
use super::prompt::Profile;
use super::{Action, MarketState, VillageQuote};
use crate::db::avatars::{self, AvatarWithConfig};
use crate::db::decisions::{self, NewDecision};
use crate::db::{villages, yield_snapshots};
use crate::indexer::{is_stale, stop_requested};

/// `decisions.source` for live simulated avatars.
const SOURCE: &str = "sim";

/// Runs until `shutdown` turns `true`. The first tick starts immediately.
/// A tick in progress always finishes before the loop stops.
pub async fn run(
    pool: PgPool,
    llm: LlmClient,
    interval: Duration,
    stale_after: Duration,
    mut shutdown: watch::Receiver<bool>,
) {
    tracing::info!(
        interval_secs = interval.as_secs(),
        model = llm.model(),
        "agent loop started"
    );
    let mut ticker = tokio::time::interval(interval);
    // A slow tick (slow LLM) delays the next one instead of piling up ticks.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match tick_once(&pool, &llm, stale_after, Utc::now()).await {
                    Ok(logged) => tracing::info!(decisions = logged, "agent tick done"),
                    Err(err) => tracing::error!("agent tick failed: {err:#}"),
                }
            }
            () = stop_requested(&mut shutdown) => break,
        }
    }
    tracing::info!("agent loop stopped");
}

/// One tick: one decision per active `sim` avatar. Returns how many decisions
/// were stored. One avatar failing (e.g. a database error) is logged and does
/// not stop the others.
///
/// # Errors
/// Only when the shared inputs (villages, avatars) cannot be loaded.
pub async fn tick_once(
    pool: &PgPool,
    llm: &LlmClient,
    stale_after: Duration,
    now: DateTime<Utc>,
) -> Result<usize> {
    let villages = live_villages(pool, now, stale_after).await?;
    let avatars = avatars::list_active_avatars(pool, SOURCE).await?;

    let mut logged = 0;
    for avatar in &avatars {
        match decide_for_avatar(pool, llm, avatar, &villages, now).await {
            Ok(()) => logged += 1,
            Err(err) => tracing::error!(avatar = %avatar.id, "agent decision failed: {err:#}"),
        }
    }
    Ok(logged)
}

async fn decide_for_avatar(
    pool: &PgPool,
    llm: &LlmClient,
    avatar: &AvatarWithConfig,
    villages: &[VillageQuote],
    now: DateTime<Utc>,
) -> Result<()> {
    let risk: Risk = avatar.risk.parse().map_err(anyhow::Error::msg)?;
    let state = MarketState {
        day: now.date_naive(),
        villages: villages.to_vec(),
        cash_usd: avatar.cash_usd,
        positions: BTreeMap::new(), // Day 4: read from the `positions` table
    };
    let ticks_since_last_move = decisions::ticks_since_last_move(pool, avatar.id, SOURCE).await?;
    let profile = Profile {
        persona: &avatar.persona,
        risk,
        goal: &avatar.goal,
    };

    let decision = decide(llm, pool, &profile, &state, ticks_since_last_move).await;

    let row = NewDecision {
        avatar_id: avatar.id,
        ts: now,
        source: SOURCE.to_string(),
        input_hash: decision.input_hash.clone(),
        model: decision.model.clone(),
        proposed_action: decision
            .proposed
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .context("could not serialize the proposed action")?,
        applied_action: serde_json::to_value(&decision.applied)
            .context("could not serialize the applied action")?,
        rejected_rule: decision.rejected_rule.clone(),
        reason_public: Some(decision.reason_public()),
        cost_usd: rust_decimal::Decimal::ZERO, // Day 4: cost model
    };
    decisions::insert_decision(pool, &row).await?;

    tracing::info!(
        avatar = %avatar.id,
        action = action_name(&decision.applied),
        rejected_rule = decision.rejected_rule.as_deref().unwrap_or("-"),
        "agent decided"
    );
    Ok(())
}

/// Villages that may receive money now: active, with a snapshot, and not
/// stale (the one stale rule, `indexer::is_stale`).
async fn live_villages(
    pool: &PgPool,
    now: DateTime<Utc>,
    stale_after: Duration,
) -> Result<Vec<VillageQuote>> {
    let mut latest: HashMap<String, yield_snapshots::YieldSnapshot> =
        yield_snapshots::latest_snapshot_per_village(pool)
            .await?
            .into_iter()
            .map(|snapshot| (snapshot.village_id.clone(), snapshot))
            .collect();

    let quotes = villages::list_active_villages(pool)
        .await?
        .into_iter()
        .filter(|village| !is_stale(village.last_fetched_at, now, stale_after))
        .filter_map(|village| {
            let snapshot = latest.remove(&village.id)?;
            Some(VillageQuote {
                village_id: village.id,
                kind: village.kind,
                apy: snapshot.apy_base + snapshot.apy_reward,
                tvl_usd: snapshot.tvl_usd,
            })
        })
        .collect();
    Ok(quotes)
}

fn action_name(action: &Action) -> &'static str {
    match action {
        Action::Hold { .. } => "HOLD",
        Action::Move { .. } => "MOVE",
        Action::Rebalance { .. } => "REBALANCE",
        Action::Say { .. } => "SAY",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::{Json, Router, extract::State, routing::post};
    use chrono::TimeZone;
    use rust_decimal::Decimal;
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::*;
    use crate::config::LlmConfig;

    const TWO_HOURS: Duration = Duration::from_secs(7200);

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 5, 12, 0, 0).unwrap()
    }

    /// A fake OpenAI-compatible server that always answers `content` and
    /// counts calls. Returns the base URL.
    async fn fake_llm(content: &str, calls: Arc<AtomicUsize>) -> String {
        let content = content.to_string();
        let app = Router::new()
            .route(
                "/chat/completions",
                post(
                    |State((content, calls)): State<(String, Arc<AtomicUsize>)>,
                     Json(_): Json<Value>| async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Json(json!({
                            "id": "chatcmpl-test",
                            "object": "chat.completion",
                            "created": 0,
                            "model": "fake-model",
                            "choices": [{
                                "index": 0,
                                "message": { "role": "assistant", "content": content },
                                "finish_reason": "stop"
                            }]
                        }))
                    },
                ),
            )
            .with_state((content, calls));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn client(base_url: &str) -> LlmClient {
        LlmClient::new(&LlmConfig {
            base_url: base_url.to_string(),
            api_key: "test-key".to_string(),
            model: "fake-model".to_string(),
        })
    }

    /// Every seeded village gets a fresh fetch and one snapshot at 10% APY,
    /// except `skip` (never fetched = stale).
    async fn seed_villages(pool: &PgPool, skip: Option<&str>) {
        for village in villages::list_active_villages(pool).await.unwrap() {
            if Some(village.id.as_str()) == skip {
                continue;
            }
            villages::mark_fetched(pool, &village.id, now())
                .await
                .unwrap();
            yield_snapshots::insert_snapshot(
                pool,
                &yield_snapshots::YieldSnapshot {
                    village_id: village.id.clone(),
                    ts: now(),
                    apy_base: Decimal::TEN,
                    apy_reward: Decimal::ZERO,
                    tvl_usd: Decimal::from(1_000_000),
                },
            )
            .await
            .unwrap();
        }
    }

    async fn create_avatar(pool: &PgPool, persona: &str, risk: &str) -> Uuid {
        let owner = "alice.testnet";
        avatars::ensure_user(pool, owner).await.unwrap();
        let config_id = avatars::insert_config(
            pool,
            &avatars::NewConfig {
                persona: persona.to_string(),
                risk: risk.to_string(),
                goal: "max_yield".to_string(),
                created_by: owner.to_string(),
            },
        )
        .await
        .unwrap();
        avatars::insert_avatar(
            pool,
            &avatars::NewAvatar {
                owner: owner.to_string(),
                config_id,
                name: "Hanna".to_string(),
                sprite: "merchant_1".to_string(),
                mode: "sim".to_string(),
                starting_capital_usd: Decimal::from(1000),
            },
        )
        .await
        .unwrap()
    }

    async fn only_decision(pool: &PgPool, avatar: Uuid) -> decisions::DecisionRow {
        let mut rows = decisions::list_decisions(pool, avatar, "sim", 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        rows.remove(0)
    }

    /// Day 3 acceptance (HANDOFF §4): 3 ticks logged for 5 avatars, each with
    /// input hash, model, action, validity and reason.
    #[sqlx::test]
    async fn three_ticks_log_a_decision_for_each_of_five_avatars(pool: PgPool) {
        seed_villages(&pool, None).await;
        let mut ids = Vec::new();
        for i in 0..5 {
            ids.push(create_avatar(&pool, &format!("Strategy number {i}."), "high").await);
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let llm = client(
            &fake_llm(
                r#"{"action":"HOLD","reason":"Yields are flat."}"#,
                calls.clone(),
            )
            .await,
        );

        for _ in 0..3 {
            assert_eq!(tick_once(&pool, &llm, TWO_HOURS, now()).await.unwrap(), 5);
        }

        for id in &ids {
            let rows = decisions::list_decisions(&pool, *id, "sim", 10)
                .await
                .unwrap();
            assert_eq!(rows.len(), 3);
            for row in rows {
                assert!(row.valid);
                assert_eq!(row.input_hash.len(), 64);
                assert_eq!(row.model, "fake-model");
                assert_eq!(row.applied_action["action"], "HOLD");
                assert_eq!(row.reason_public.as_deref(), Some("Yields are flat."));
            }
        }
        // Same input on ticks 2 and 3: answered from the cache, so one call
        // per avatar (each has its own persona, so its own cache key).
        assert_eq!(calls.load(Ordering::SeqCst), 5);
    }

    #[sqlx::test]
    async fn unparseable_answer_is_retried_once_then_held(pool: PgPool) {
        seed_villages(&pool, None).await;
        let avatar = create_avatar(&pool, "Lend.", "low").await;
        let calls = Arc::new(AtomicUsize::new(0));
        let llm = client(&fake_llm("I think holding is wise.", calls.clone()).await);

        tick_once(&pool, &llm, TWO_HOURS, now()).await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let row = only_decision(&pool, avatar).await;
        assert!(!row.valid);
        assert_eq!(row.rejected_rule.as_deref(), Some("parse_error"));
        assert_eq!(row.proposed_action, None);
        assert_eq!(row.applied_action["action"], "HOLD");
    }

    #[sqlx::test]
    async fn unreachable_llm_gives_hold_with_llm_error(pool: PgPool) {
        seed_villages(&pool, None).await;
        let avatar = create_avatar(&pool, "Lend.", "low").await;
        // Port 9 (discard) on localhost: nothing listens, connection refused.
        let llm = client("http://127.0.0.1:9");

        tick_once(&pool, &llm, TWO_HOURS, now()).await.unwrap();

        let row = only_decision(&pool, avatar).await;
        assert_eq!(row.rejected_rule.as_deref(), Some("llm_error"));
    }

    #[sqlx::test]
    async fn money_cannot_go_to_a_stale_village(pool: PgPool) {
        seed_villages(&pool, Some("rhea-dex-frax-usdc")).await;
        let avatar = create_avatar(&pool, "Chase FRAX.", "high").await;
        let answer =
            r#"{"action":"REBALANCE","weights":{"rhea-dex-frax-usdc":100},"reason":"FRAX pays."}"#;
        let llm = client(&fake_llm(answer, Arc::new(AtomicUsize::new(0))).await);

        tick_once(&pool, &llm, TWO_HOURS, now()).await.unwrap();

        let row = only_decision(&pool, avatar).await;
        assert_eq!(row.rejected_rule.as_deref(), Some("village_unavailable"));
        assert_eq!(row.proposed_action.unwrap()["action"], "REBALANCE");
        assert_eq!(row.applied_action["action"], "HOLD");
    }

    #[sqlx::test]
    async fn allowed_rebalance_is_logged_and_starts_the_cooldown(pool: PgPool) {
        seed_villages(&pool, None).await;
        let avatar = create_avatar(&pool, "Split lending.", "low").await;
        let answer = r#"{"action":"REBALANCE","weights":{"rhea-lend-usdc":50,"rhea-lend-usdt":50},"reason":"Split."}"#;
        let llm = client(&fake_llm(answer, Arc::new(AtomicUsize::new(0))).await);

        tick_once(&pool, &llm, TWO_HOURS, now()).await.unwrap();
        tick_once(&pool, &llm, TWO_HOURS, now()).await.unwrap();

        let rows = decisions::list_decisions(&pool, avatar, "sim", 10)
            .await
            .unwrap();
        // Newest first: the second tick is refused by the low-risk cooldown.
        assert_eq!(rows[1].applied_action["action"], "REBALANCE");
        assert!(rows[1].valid);
        assert_eq!(rows[0].rejected_rule.as_deref(), Some("cooldown"));
    }

    #[sqlx::test]
    async fn persona_is_never_stored_in_a_decision(pool: PgPool) {
        seed_villages(&pool, None).await;
        let persona = "Secret plan: always buy the dip in FRAX.";
        let avatar = create_avatar(&pool, persona, "high").await;
        let llm = client(
            &fake_llm(
                r#"{"action":"HOLD","reason":"Wait."}"#,
                Arc::new(AtomicUsize::new(0)),
            )
            .await,
        );

        tick_once(&pool, &llm, TWO_HOURS, now()).await.unwrap();

        let row = only_decision(&pool, avatar).await;
        let stored = format!("{row:?}");
        assert!(!stored.contains("Secret plan"));
    }

    #[sqlx::test]
    async fn inactive_avatars_are_skipped(pool: PgPool) {
        seed_villages(&pool, None).await;
        let llm = client(
            &fake_llm(
                r#"{"action":"HOLD","reason":"x"}"#,
                Arc::new(AtomicUsize::new(0)),
            )
            .await,
        );
        let active = create_avatar(&pool, "A.", "low").await;
        let inactive = create_avatar(&pool, "B.", "low").await;
        sqlx::query("UPDATE avatars SET active = false WHERE id = $1")
            .bind(inactive)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(tick_once(&pool, &llm, TWO_HOURS, now()).await.unwrap(), 1);
        only_decision(&pool, active).await;
    }
}
