//! The live tick loop (TECH §5.4): every `AGENT_TICK_SECS`, each active `sim`
//! avatar gets one decision, and its paper money really moves.
//!
//! Per avatar (Day 4):
//!
//! ```text
//! load cash + positions ─> accrue yield since the last sim equity point (D33)
//!   ─> decide (LLM + guardrails + cost) ─> apply on the in-memory ledger
//!   ─> ONE transaction: positions, cash, decision row, sim equity point (D34)
//! ```
//!
//! The LLM call happens before the transaction opens, so no database lock is
//! held while waiting for a model. Only this loop writes `positions` and
//! ticks run one after the other, so two writers cannot race (one backend
//! process, D11).
//!
//! Avatars are handled one after the other. With 5 to 10 demo avatars and a
//! 10-minute tick that is plenty; parallel calls can come later if needed.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use super::decide::{Decision, decide};
use super::guardrails::Risk;
use super::llm::LlmClient;
use super::prompt::Profile;
use super::{Action, MarketState, VillageQuote};
use crate::db::avatars::{self, AvatarWithConfig};
use crate::db::decisions::{self, NewDecision};
use crate::db::equity::{self, EquityPoint};
use crate::db::{positions, villages, yield_snapshots};
use crate::indexer::{is_stale, stop_requested};
use crate::live_events::{LiveEvent, LiveSender, main_village, publish};
use crate::sim::ledger::Ledger;

/// `decisions.source` and `equity_snapshots.source` for live simulated avatars.
const SOURCE: &str = "sim";

/// `rejected_rule` when the ledger refused an action the guardrails allowed.
/// Should never happen (both check the same limits); holding is the safe answer.
const LEDGER_ERROR: &str = "ledger_error";

/// What every avatar sees in one tick.
struct LiveMarket {
    /// Villages that may receive money now (fresh, not stale).
    quotes: Vec<VillageQuote>,
    /// Latest APY of every active village, stale ones included: money already
    /// in a stale village keeps earning its last known APY (D23).
    last_apys: BTreeMap<String, Decimal>,
}

/// Runs until `shutdown` turns `true`. The first tick starts immediately.
/// A tick in progress always finishes before the loop stops.
pub async fn run(
    pool: PgPool,
    llm: LlmClient,
    interval: Duration,
    stale_after: Duration,
    events: LiveSender,
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
                match tick_once(&pool, &llm, stale_after, &events, Utc::now()).await {
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
/// not stop the others. Each stored decision is announced on `events` (D43).
///
/// # Errors
/// Only when the shared inputs (villages, avatars) cannot be loaded.
pub async fn tick_once(
    pool: &PgPool,
    llm: &LlmClient,
    stale_after: Duration,
    events: &LiveSender,
    now: DateTime<Utc>,
) -> Result<usize> {
    let market = live_market(pool, now, stale_after).await?;
    let avatars = avatars::list_active_avatars(pool, SOURCE).await?;

    let mut logged = 0;
    for avatar in &avatars {
        match tick_avatar(pool, llm, avatar, &market, events, now).await {
            Ok(()) => logged += 1,
            Err(err) => tracing::error!(avatar = %avatar.id, "agent decision failed: {err:#}"),
        }
    }
    Ok(logged)
}

/// One avatar's tick. Nothing is written unless every step succeeds: the
/// four writes share one transaction. Events are published only after the
/// commit, so a browser never hears about a tick that was rolled back.
async fn tick_avatar(
    pool: &PgPool,
    llm: &LlmClient,
    avatar: &AvatarWithConfig,
    market: &LiveMarket,
    events: &LiveSender,
    now: DateTime<Utc>,
) -> Result<()> {
    let risk: Risk = avatar.risk.parse().map_err(anyhow::Error::msg)?;

    // 1. Rebuild the ledger and grow it for the time since the last tick.
    let stored_positions = positions::load_positions(pool, avatar.id).await?;
    // Where the sprite stood before this tick (for `avatar_move`).
    let village_before = main_village(&stored_positions);
    let mut ledger = Ledger::from_parts(avatar.cash_usd, stored_positions);
    if let Some(last) = equity::last_equity_ts(pool, avatar.id, SOURCE).await? {
        // A negative gap (clock moved back) accrues nothing.
        if let Ok(seconds) = u64::try_from((now - last).num_seconds()) {
            ledger.accrue_seconds(&market.last_apys, seconds)?;
        }
    }

    // 2. Decide on the grown state (LLM call, outside any transaction).
    let state = MarketState {
        day: now.date_naive(),
        villages: market.quotes.clone(),
        cash_usd: ledger.cash_usd(),
        positions: ledger.positions().clone(),
    };
    let ticks_since_last_move = decisions::ticks_since_last_move(pool, avatar.id, SOURCE).await?;
    let profile = Profile {
        persona: &avatar.persona,
        risk,
        goal: &avatar.goal,
    };
    let mut decision = decide(llm, pool, &profile, &state, ticks_since_last_move).await;

    // 3. Apply it to the ledger, paying its cost.
    if let Err(err) = ledger.apply_action(&decision.applied, decision.cost_usd) {
        tracing::error!(avatar = %avatar.id, "ledger refused an allowed action ({err}), holding");
        hold_after_ledger_error(&mut decision);
    }

    // 4. Store everything in one transaction.
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
        cost_usd: decision.cost_usd,
        // Stored only; the `LiveEvent` below deliberately leaves it out.
        error_detail: decision.error_detail.clone(),
    };
    let mut tx = pool.begin().await?;
    positions::replace_positions(&mut tx, avatar.id, ledger.positions()).await?;
    avatars::update_cash(&mut *tx, avatar.id, ledger.cash_usd()).await?;
    decisions::insert_decision(&mut *tx, &row).await?;
    let point = EquityPoint {
        ts: now,
        equity_usd: ledger.equity(),
    };
    equity::insert_equity_point(&mut *tx, avatar.id, SOURCE, &point).await?;
    // An early `?` above drops `tx` without commit: sqlx rolls it all back.
    tx.commit().await?;

    // 5. Tell the browsers (public fields only: no persona, D43).
    publish(
        events,
        LiveEvent::Decision {
            avatar_id: avatar.id,
            ts: now,
            action: row.applied_action.clone(),
            valid: row.rejected_rule.is_none(),
            rejected_rule: row.rejected_rule.clone(),
            reason_public: row.reason_public.clone(),
            cost_usd: row.cost_usd,
            equity_usd: point.equity_usd,
        },
    );
    let village_after = main_village(ledger.positions());
    if village_after != village_before {
        publish(
            events,
            LiveEvent::AvatarMove {
                avatar_id: avatar.id,
                from: village_before,
                to: village_after,
            },
        );
    }

    tracing::info!(
        avatar = %avatar.id,
        action = action_name(&decision.applied),
        rejected_rule = decision.rejected_rule.as_deref().unwrap_or("-"),
        cost_usd = %decision.cost_usd,
        equity_usd = %point.equity_usd,
        "agent decided"
    );
    Ok(())
}

/// Turns an allowed decision into HOLD after the ledger refused it, keeping
/// the proposal so the row shows what was asked.
fn hold_after_ledger_error(decision: &mut Decision) {
    decision.applied = Action::Hold {
        reason: format!("Holding: proposal refused ({LEDGER_ERROR})."),
    };
    decision.rejected_rule = Some(LEDGER_ERROR.to_string());
    decision.cost_usd = Decimal::ZERO;
}

/// Loads what every avatar sees this tick.
///
/// `quotes`: villages that may receive money now: active, with a snapshot,
/// and not stale (the one stale rule, `indexer::is_stale`).
/// `last_apys`: the latest APY of every active village with a snapshot.
async fn live_market(
    pool: &PgPool,
    now: DateTime<Utc>,
    stale_after: Duration,
) -> Result<LiveMarket> {
    let latest: HashMap<String, yield_snapshots::YieldSnapshot> =
        yield_snapshots::latest_snapshot_per_village(pool)
            .await?
            .into_iter()
            .map(|snapshot| (snapshot.village_id.clone(), snapshot))
            .collect();

    let mut market = LiveMarket {
        quotes: Vec::new(),
        last_apys: BTreeMap::new(),
    };
    for village in villages::list_active_villages(pool).await? {
        let Some(snapshot) = latest.get(&village.id) else {
            continue; // never fetched: no data at all
        };
        let apy = snapshot.apy_base + snapshot.apy_reward;
        market.last_apys.insert(village.id.clone(), apy);
        if !is_stale(village.last_fetched_at, now, stale_after) {
            market
                .quotes
                .push(VillageQuote::new(&village, apy, snapshot.tvl_usd));
        }
    }
    Ok(market)
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
    use rust_decimal::{Decimal, MathematicalOps};
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::*;
    use crate::config::LlmConfig;
    use crate::live_events::channel;

    const TWO_HOURS: Duration = Duration::from_secs(7200);

    /// Every event published so far on a subscribed receiver.
    fn drain(receiver: &mut tokio::sync::broadcast::Receiver<LiveEvent>) -> Vec<LiveEvent> {
        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }
        events
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 5, 12, 0, 0).unwrap()
    }

    /// `ticks` ticks of 10 minutes after `now()`. Each tick stores one sim
    /// equity point, so two ticks of one avatar need different times.
    fn later(ticks: i64) -> DateTime<Utc> {
        now() + chrono::Duration::minutes(10 * ticks)
    }

    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    async fn cash(pool: &PgPool, avatar: Uuid) -> Decimal {
        avatars::get_avatar_with_config(pool, avatar)
            .await
            .unwrap()
            .unwrap()
            .cash_usd
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
                active: true,
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

        for tick in 0..3 {
            assert_eq!(
                tick_once(&pool, &llm, TWO_HOURS, &channel(), later(tick))
                    .await
                    .unwrap(),
                5
            );
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
                assert_eq!(row.error_detail, None);
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

        tick_once(&pool, &llm, TWO_HOURS, &channel(), now())
            .await
            .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let row = only_decision(&pool, avatar).await;
        assert!(!row.valid);
        assert_eq!(row.rejected_rule.as_deref(), Some("parse_error"));
        assert_eq!(row.proposed_action, None);
        assert_eq!(row.applied_action["action"], "HOLD");
        // Both attempts are explained, without the model's own words.
        let detail = row
            .error_detail
            .expect("parse_error keeps the error detail");
        assert!(
            detail.contains("attempt 1: LLM answer is not a valid action"),
            "{detail}"
        );
        assert!(
            detail.contains("attempt 2: LLM answer is not a valid action"),
            "{detail}"
        );
        assert!(!detail.contains("holding is wise"), "{detail}");
    }

    #[sqlx::test]
    async fn unreachable_llm_gives_hold_with_llm_error(pool: PgPool) {
        seed_villages(&pool, None).await;
        let avatar = create_avatar(&pool, "Lend.", "low").await;
        // Port 9 (discard) on localhost: nothing listens, connection refused.
        let llm = client("http://127.0.0.1:9");

        tick_once(&pool, &llm, TWO_HOURS, &channel(), now())
            .await
            .unwrap();

        let row = only_decision(&pool, avatar).await;
        assert_eq!(row.rejected_rule.as_deref(), Some("llm_error"));
        // The exact error of each attempt is kept, not only the label.
        let detail = row.error_detail.expect("llm_error keeps the error detail");
        assert!(detail.contains("attempt 1: LLM call failed"), "{detail}");
        assert!(detail.contains("attempt 2: LLM call failed"), "{detail}");
    }

    #[sqlx::test]
    async fn money_cannot_go_to_a_stale_village(pool: PgPool) {
        seed_villages(&pool, Some("rhea-dex-frax-usdc")).await;
        let avatar = create_avatar(&pool, "Chase FRAX.", "high").await;
        let answer =
            r#"{"action":"REBALANCE","weights":{"rhea-dex-frax-usdc":100},"reason":"FRAX pays."}"#;
        let llm = client(&fake_llm(answer, Arc::new(AtomicUsize::new(0))).await);

        tick_once(&pool, &llm, TWO_HOURS, &channel(), now())
            .await
            .unwrap();

        let row = only_decision(&pool, avatar).await;
        assert_eq!(row.rejected_rule.as_deref(), Some("village_unavailable"));
        assert_eq!(row.proposed_action.unwrap()["action"], "REBALANCE");
        assert_eq!(row.applied_action["action"], "HOLD");
        // A refused action moves no money and costs nothing.
        assert_eq!(row.cost_usd, Decimal::ZERO);
        assert_eq!(cash(&pool, avatar).await, d("1000"));
        assert!(
            positions::load_positions(&pool, avatar)
                .await
                .unwrap()
                .is_empty()
        );
        let points = equity::list_equity(&pool, avatar, "sim").await.unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].equity_usd, d("1000"));
    }

    /// Day 4: an allowed REBALANCE really moves the paper money, pays its
    /// cost, and the next tick grows it with 10 minutes of yield.
    #[sqlx::test]
    async fn an_allowed_rebalance_moves_the_paper_money_and_then_earns(pool: PgPool) {
        seed_villages(&pool, None).await;
        let avatar = create_avatar(&pool, "Split lending.", "low").await;
        let answer = r#"{"action":"REBALANCE","weights":{"rhea-lend-usdc":50,"rhea-lend-usdt":50},"reason":"Split."}"#;
        let llm = client(&fake_llm(answer, Arc::new(AtomicUsize::new(0))).await);

        tick_once(&pool, &llm, TWO_HOURS, &channel(), now())
            .await
            .unwrap();

        // Cost: 2 lending entries (2 × 0.02 gas) + the USDC -> USDT swap of
        // 500 through the stable pool (0.10 fee + 0.025 slippage + 0.02 gas).
        let first = only_decision(&pool, avatar).await;
        assert_eq!(first.cost_usd, d("0.185"));
        assert_eq!(cash(&pool, avatar).await, Decimal::ZERO);
        let held = positions::load_positions(&pool, avatar).await.unwrap();
        assert_eq!(held["rhea-lend-usdc"], d("499.9075"));
        assert_eq!(held["rhea-lend-usdt"], d("499.9075"));

        // Second tick: refused by the low-risk cooldown, but the money earns.
        tick_once(&pool, &llm, TWO_HOURS, &channel(), later(1))
            .await
            .unwrap();

        let points = equity::list_equity(&pool, avatar, "sim").await.unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].equity_usd, d("999.815"));
        // 10% APY for 600 seconds: × 1.1^(600 / 31 536 000).
        let factor = d("1.1").powd(d("600") / d("31536000"));
        assert_eq!(
            points[1].equity_usd.round_dp(10),
            (d("999.815") * factor).round_dp(10)
        );
        assert!(points[1].equity_usd > points[0].equity_usd);
    }

    #[sqlx::test]
    async fn the_first_tick_of_an_avatar_does_not_accrue(pool: PgPool) {
        seed_villages(&pool, None).await;
        let avatar = create_avatar(&pool, "Wait.", "low").await;
        let llm = client(
            &fake_llm(
                r#"{"action":"HOLD","reason":"Wait."}"#,
                Arc::new(AtomicUsize::new(0)),
            )
            .await,
        );

        tick_once(&pool, &llm, TWO_HOURS, &channel(), now())
            .await
            .unwrap();

        let points = equity::list_equity(&pool, avatar, "sim").await.unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].ts, now());
        assert_eq!(points[0].equity_usd, d("1000"));
    }

    #[sqlx::test]
    async fn allowed_rebalance_is_logged_and_starts_the_cooldown(pool: PgPool) {
        seed_villages(&pool, None).await;
        let avatar = create_avatar(&pool, "Split lending.", "low").await;
        let answer = r#"{"action":"REBALANCE","weights":{"rhea-lend-usdc":50,"rhea-lend-usdt":50},"reason":"Split."}"#;
        let llm = client(&fake_llm(answer, Arc::new(AtomicUsize::new(0))).await);

        tick_once(&pool, &llm, TWO_HOURS, &channel(), now())
            .await
            .unwrap();
        tick_once(&pool, &llm, TWO_HOURS, &channel(), later(1))
            .await
            .unwrap();

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

        tick_once(&pool, &llm, TWO_HOURS, &channel(), now())
            .await
            .unwrap();

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

        assert_eq!(
            tick_once(&pool, &llm, TWO_HOURS, &channel(), now())
                .await
                .unwrap(),
            1
        );
        only_decision(&pool, active).await;
    }

    /// D43: a move is announced as `decision` then `avatar_move` (cash ->
    /// the largest position; a 50/50 tie goes to the smallest village id).
    #[sqlx::test]
    async fn a_move_publishes_a_decision_and_an_avatar_move(pool: PgPool) {
        seed_villages(&pool, None).await;
        let persona = "Secret plan: split lending.";
        let avatar = create_avatar(&pool, persona, "low").await;
        let answer = r#"{"action":"REBALANCE","weights":{"rhea-lend-usdc":50,"rhea-lend-usdt":50},"reason":"Split."}"#;
        let llm = client(&fake_llm(answer, Arc::new(AtomicUsize::new(0))).await);
        let events = channel();
        let mut receiver = events.subscribe();

        tick_once(&pool, &llm, TWO_HOURS, &events, now())
            .await
            .unwrap();

        let published = drain(&mut receiver);
        assert_eq!(published.len(), 2, "{published:?}");
        let LiveEvent::Decision {
            avatar_id,
            valid,
            cost_usd,
            ref action,
            ..
        } = published[0]
        else {
            panic!("expected a decision first, got {:?}", published[0]);
        };
        assert_eq!(avatar_id, avatar);
        assert!(valid);
        assert_eq!(cost_usd, d("0.185"));
        assert_eq!(action["action"], "REBALANCE");
        assert_eq!(
            published[1],
            LiveEvent::AvatarMove {
                avatar_id: avatar,
                from: None,
                to: Some("rhea-lend-usdc".to_string()),
            }
        );
        let serialized = serde_json::to_string(&published).unwrap();
        assert!(!serialized.contains("Secret plan"), "no persona in events");
    }

    #[sqlx::test]
    async fn a_hold_publishes_only_a_decision(pool: PgPool) {
        seed_villages(&pool, None).await;
        create_avatar(&pool, "Wait.", "low").await;
        let llm = client(
            &fake_llm(
                r#"{"action":"HOLD","reason":"Wait."}"#,
                Arc::new(AtomicUsize::new(0)),
            )
            .await,
        );
        let events = channel();
        let mut receiver = events.subscribe();

        tick_once(&pool, &llm, TWO_HOURS, &events, now())
            .await
            .unwrap();

        let published = drain(&mut receiver);
        assert_eq!(published.len(), 1);
        assert!(matches!(
            &published[0],
            LiveEvent::Decision { equity_usd, .. } if *equity_usd == d("1000")
        ));
    }
}
