//! Backtest: replay the last 7 days of real yield history with an avatar's
//! decision rule and store the resulting equity curve, labeled `backtest`
//! (SPEC F5, TECH §5.2).
//!
//! Two layers:
//! - `run` has no database or network (history in, equity points out); the
//!   decision rule is passed in, so the replay rules are unit-tested with
//!   simple closures.
//! - `run_for_avatar` loads the avatar and history from Postgres, calls `run`
//!   with the **live** `agent::decide` (LLM + persona, D63) and saves the
//!   equity and the decisions in one transaction (D24).
//!
//! Each day: build the `MarketState`, ask `decide`, check the action with the
//! guardrails (a refused action becomes HOLD), apply it paying its estimated
//! cost (SPEC F5.3, same cost model as the live sim), grow positions by one
//! day of yield, record equity. One day = one tick for the cooldown rule.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Days, NaiveDate, NaiveTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::agent::decide::{Decision, decide};
use crate::agent::guardrails::{self, Risk};
use crate::agent::llm::LlmClient;
use crate::agent::prompt::Profile;
use crate::agent::{Action, MarketState, VillageQuote};
use crate::db::decisions::{self, NewDecision};
use crate::db::equity::{self, EquityPoint};
use crate::db::villages::Village;
use crate::db::{avatars, villages, yield_snapshots};
use crate::sim::ledger::{Ledger, LedgerError};

/// `decisions.source` / `equity_snapshots.source` of every backtest row.
const SOURCE: &str = "backtest";

/// Length of the backtest window (SPEC F5.1, D60). Kept short because each
/// day is one `decide()` call, which costs AI tokens once the LLM is used.
pub const BACKTEST_DAYS: u64 = 7;

/// The villages that have data on one UTC day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayQuotes {
    pub day: NaiveDate,
    /// Empty when no village has data that day.
    pub villages: Vec<VillageQuote>,
}

/// The replay rules of one backtest, one day at a time. Each day the caller
/// asks `morning_state` + `ticks_since_last_move`, gets an action from its
/// decision rule, and gives it to `finish_day`.
///
/// Why a struct and not a callback: the LLM rule is async, and an async
/// callback that borrows the LLM client cannot be proven `Send` by the
/// compiler (needed by axum handlers). With this shape, `run` (plain rule,
/// tests) and `run_for_avatar` (LLM) share every rule and differ only in how
/// they ask for the action.
struct Replay {
    ledger: Ledger,
    risk: Risk,
    points: Vec<EquityPoint>,
    /// Index of the day being replayed.
    day_index: usize,
    /// Index of the day of the last applied MOVE or REBALANCE (cooldown).
    last_move_day: Option<usize>,
}

impl Replay {
    fn new(starting_capital: Decimal, risk: Risk) -> Self {
        Self {
            ledger: Ledger::new(starting_capital),
            risk,
            points: Vec::new(),
            day_index: 0,
            last_move_day: None,
        }
    }

    /// What the decision rule sees at the start of `today`.
    fn morning_state(&self, today: &DayQuotes) -> MarketState {
        MarketState {
            day: today.day,
            villages: today.villages.clone(),
            cash_usd: self.ledger.cash_usd(),
            positions: self.ledger.positions().clone(),
        }
    }

    /// Days since the last applied move, `None` when it never moved. The
    /// cooldown rule reads it, and the LLM prompt shows it.
    fn ticks_since_last_move(&self) -> Option<u32> {
        self.last_move_day.map(|day| (self.day_index - day) as u32)
    }

    /// Applies `action` (guardrails first, HOLD when refused), grows the
    /// positions by one day of yield and records the day's equity point.
    fn finish_day(
        &mut self,
        today: &DayQuotes,
        state: &MarketState,
        action: &Action,
    ) -> Result<(), LedgerError> {
        let ticks = self.ticks_since_last_move();
        if apply_action(&mut self.ledger, action, state, self.risk, ticks) {
            self.last_move_day = Some(self.day_index);
        }

        let apys: BTreeMap<String, Decimal> = today
            .villages
            .iter()
            .map(|quote| (quote.village_id.clone(), quote.apy))
            .collect();
        self.ledger.accrue_one_day(&apys)?;

        self.points.push(EquityPoint {
            ts: end_of_day(today.day),
            equity_usd: self.ledger.equity(),
        });
        self.day_index += 1;
        Ok(())
    }
}

/// Replays `days` in order with a plain decision rule, starting with
/// `starting_capital` in cash, and returns one equity point per day,
/// timestamped at the **end** of that day (next UTC midnight).
///
/// Every action goes through `guardrails::check` for `risk` ("the LLM
/// proposes, code decides"). A refused action becomes HOLD and is logged with
/// the rule. A village without data that day is not in `villages`, so it
/// cannot receive money (D23).
///
/// `decide` gets the day's state and the days since the last applied move
/// (`None` = never moved). `run_for_avatar` runs the same loop with the LLM.
///
/// # Errors
/// `LedgerError::Overflow` if an amount no longer fits a `Decimal`.
pub fn run(
    days: &[DayQuotes],
    starting_capital: Decimal,
    risk: Risk,
    mut decide: impl FnMut(&MarketState, Option<u32>) -> Action,
) -> Result<Vec<EquityPoint>, LedgerError> {
    let mut replay = Replay::new(starting_capital, risk);
    for today in days {
        let state = replay.morning_state(today);
        let action = decide(&state, replay.ticks_since_last_move());
        replay.finish_day(today, &state, &action)?;
    }
    Ok(replay.points)
}

/// Applies `action` to the ledger, with its estimated cost, if the
/// guardrails allow it; otherwise the ledger is unchanged (= HOLD) and the
/// refusal is logged with the rule.
///
/// Returns `true` when money was moved (MOVE or REBALANCE applied), so the
/// caller can start the cooldown.
fn apply_action(
    ledger: &mut Ledger,
    action: &Action,
    state: &MarketState,
    risk: Risk,
    ticks_since_last_move: Option<u32>,
) -> bool {
    let cost_usd = match guardrails::check(action, state, risk, ticks_since_last_move) {
        Ok(cost_usd) => cost_usd,
        Err(rule) => {
            tracing::warn!(day = %state.day, %rule, "backtest: action refused by guardrails, holding");
            return false;
        }
    };
    match ledger.apply_action(action, cost_usd) {
        Ok(moved) => moved,
        Err(err) => {
            // The guardrails already checked the same limits, so this means
            // the two disagree: a bug, but holding is still the safe answer.
            tracing::error!(day = %state.day, "backtest: ledger refused an allowed action ({err}), holding");
            false
        }
    }
}

/// Next UTC midnight after `day`: the moment a day's result is known.
fn end_of_day(day: NaiveDate) -> DateTime<Utc> {
    start_of_day(day + Days::new(1))
}

fn start_of_day(day: NaiveDate) -> DateTime<Utc> {
    day.and_time(NaiveTime::MIN).and_utc()
}

/// Runs the backtest of one avatar over the last `BACKTEST_DAYS` **complete**
/// UTC days before `now`, and replaces its stored backtest equity and
/// decisions (D24, D63).
///
/// Each day is one call of the live `agent::decide` with the avatar's persona
/// (HANDOFF D35, D63): same prompt, cache, retry, HOLD fallback and
/// guardrails as the tick loop ("one decide() for everything"). An LLM
/// failure on a day becomes HOLD `llm_error` for that day only; the next day
/// asks again (Ayoub, S1). Cost: up to `BACKTEST_DAYS` × 2 LLM calls, fewer
/// when answers are already in `llm_cache`.
///
/// Starts from the avatar's `starting_capital_usd` (the backtest is a "what
/// if you had started 7 days ago", not its current cash). `now` is a
/// parameter so tests are deterministic.
///
/// The replay starts at the first day that has data for any village, and then
/// covers every calendar day up to yesterday: a day without any data still
/// gets an equity point (money grows at the last known APY, D23). With less
/// than 7 days of history it runs the days it has and logs a warning.
///
/// Returns the stored points.
///
/// # Errors
/// Unknown avatar, no history at all in the window, a database error, or a
/// ledger overflow.
pub async fn run_for_avatar(
    pool: &PgPool,
    llm: &LlmClient,
    avatar_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<EquityPoint>> {
    let avatar = avatars::get_avatar_with_config(pool, avatar_id)
        .await?
        .with_context(|| format!("avatar {avatar_id} not found"))?;

    let window_end = start_of_day(now.date_naive()); // today's partial day is excluded
    let window_start = window_end - Days::new(BACKTEST_DAYS);
    let days = load_days(pool, window_start, window_end).await?;
    if days.is_empty() {
        bail!("no yield history between {window_start} and {window_end}");
    }
    if days.len() < BACKTEST_DAYS as usize {
        tracing::warn!(
            avatar = %avatar_id,
            days = days.len(),
            "backtest: less than {BACKTEST_DAYS} days of history, running a shorter backtest"
        );
    }

    let risk: Risk = avatar.risk.parse().map_err(anyhow::Error::msg)?;
    let profile = Profile {
        persona: &avatar.persona,
        risk,
        goal: &avatar.goal,
    };
    // Same loop as `run`, but each day asks the live `decide` (LLM). Days run
    // one after another: each starts from the previous day's ledger.
    let mut replay = Replay::new(avatar.starting_capital_usd, risk);
    let mut day_decisions: Vec<(NaiveDate, Decision)> = Vec::with_capacity(days.len());
    for today in &days {
        let state = replay.morning_state(today);
        let decision = decide(llm, pool, &profile, &state, replay.ticks_since_last_move()).await;
        replay.finish_day(today, &state, &decision.applied)?;
        day_decisions.push((today.day, decision));
    }
    let points = replay.points;

    // The LLM calls are done: only now open the transaction, so it is never
    // held open while waiting for the network.
    let mut tx = pool.begin().await?;
    equity::replace_backtest_equity(&mut tx, avatar_id, &points).await?;
    decisions::delete_backtest_decisions(&mut *tx, avatar_id).await?;
    for (day, decision) in &day_decisions {
        let row = backtest_decision_row(avatar_id, *day, decision)?;
        decisions::insert_decision(&mut *tx, &row).await?;
    }
    // An early `?` above drops `tx` without commit: sqlx rolls it all back.
    tx.commit().await?;

    tracing::info!(
        avatar = %avatar_id,
        days = points.len(),
        final_equity = %points.last().map(|p| p.equity_usd).unwrap_or_default(),
        "backtest stored"
    );
    Ok(points)
}

/// The `decisions` row of one replayed day, stamped at the **start** of that
/// day (its equity point is at the end). Same fields as a live decision.
fn backtest_decision_row(
    avatar_id: Uuid,
    day: NaiveDate,
    decision: &Decision,
) -> Result<NewDecision> {
    Ok(NewDecision {
        avatar_id,
        ts: start_of_day(day),
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
    })
}

/// Loads the daily history and turns it into one `DayQuotes` per calendar day,
/// from the first day with any data up to the day before `window_end`.
async fn load_days(
    pool: &PgPool,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Result<Vec<DayQuotes>> {
    let villages_by_id: BTreeMap<String, Village> = villages::list_active_villages(pool)
        .await?
        .into_iter()
        .map(|village| (village.id.clone(), village))
        .collect();

    let mut by_day: BTreeMap<NaiveDate, Vec<VillageQuote>> = BTreeMap::new();
    for snapshot in yield_snapshots::daily_history(pool, window_start, window_end).await? {
        // `daily_history` only returns active villages, so the village is
        // known; skip defensively if one was deactivated between the queries.
        let Some(village) = villages_by_id.get(&snapshot.village_id) else {
            continue;
        };
        by_day
            .entry(snapshot.ts.date_naive())
            .or_default()
            .push(VillageQuote::new(
                village,
                snapshot.apy_base + snapshot.apy_reward,
                snapshot.tvl_usd,
            ));
    }

    let Some(&first_day) = by_day.keys().next() else {
        return Ok(Vec::new());
    };
    let last_day = window_end.date_naive() - Days::new(1);

    let mut days = Vec::new();
    let mut day = first_day;
    while day <= last_day {
        days.push(DayQuotes {
            day,
            villages: by_day.remove(&day).unwrap_or_default(),
        });
        day = day + Days::new(1);
    }
    Ok(days)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::agent::mock_decide;
    use crate::config::LlmConfig;
    use axum::{Json, Router, extract::State, routing::post};
    use chrono::TimeZone;
    use rust_decimal::MathematicalOps;
    use serde_json::{Value, json};

    /// Turns a rule that ignores the cooldown counter into what `run` takes.
    fn sync(
        mut rule: impl FnMut(&MarketState) -> Action,
    ) -> impl FnMut(&MarketState, Option<u32>) -> Action {
        move |state: &MarketState, _: Option<u32>| rule(state)
    }

    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    fn quote(village_id: &str, apy: &str) -> VillageQuote {
        VillageQuote {
            village_id: village_id.to_string(),
            kind: "lending".to_string(),
            apy: d(apy),
            tvl_usd: d("1000000"),
            fee_bps: 0,
            tokens: vec!["USDC".to_string()],
        }
    }

    /// Gas of one lending entry or exit (`sim::costs::GAS_USD_PER_TX`): the
    /// only cost between the USDC lending villages of these tests.
    fn gas() -> Decimal {
        crate::sim::costs::GAS_USD_PER_TX
    }

    /// The starting 10 000 after the first investment from cash (one entry).
    fn invested() -> Decimal {
        d("10000") - gas()
    }

    fn date(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    /// `count` days from 1 Oct, built by `quotes_for(day_index)`.
    fn days(count: u32, quotes_for: impl Fn(u32) -> Vec<VillageQuote>) -> Vec<DayQuotes> {
        (0..count)
            .map(|i| DayQuotes {
                day: date(10, 1) + Days::new(u64::from(i)),
                villages: quotes_for(i),
            })
            .collect()
    }

    /// `value` grown for `days` days at `apy` percent, same formula as the ledger.
    fn grown(value: Decimal, apy: &str, days: i64) -> Decimal {
        let daily =
            (Decimal::ONE + d(apy) / Decimal::ONE_HUNDRED).powd(Decimal::ONE / Decimal::from(365));
        value * daily.powi(days)
    }

    fn within_a_cent(a: Decimal, b: Decimal) -> bool {
        (a - b).abs() <= d("0.01")
    }

    fn hold() -> Action {
        Action::Hold {
            reason: "test".to_string(),
        }
    }

    #[test]
    fn seven_days_give_seven_points_stamped_at_end_of_day() {
        let history = days(7, |_| vec![quote("a", "10")]);

        let points = run(&history, d("10000"), Risk::High, sync(mock_decide)).unwrap();

        assert_eq!(points.len(), 7);
        assert_eq!(
            points[0].ts,
            Utc.with_ymd_and_hms(2026, 10, 2, 0, 0, 0).unwrap()
        );
        assert_eq!(
            points[6].ts,
            Utc.with_ymd_and_hms(2026, 10, 8, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn flat_apy_grows_equity_every_day_by_the_formula() {
        let history = days(7, |_| vec![quote("a", "10")]);

        let points = run(&history, d("10000"), Risk::High, sync(mock_decide)).unwrap();

        assert!(
            points
                .windows(2)
                .all(|pair| pair[1].equity_usd > pair[0].equity_usd)
        );
        // Day 0 invests the cash, paying one entry of gas.
        assert!(within_a_cent(
            points[6].equity_usd,
            grown(invested(), "10", 7)
        ));
    }

    #[test]
    fn mock_follows_the_best_village_when_it_changes() {
        // Days 0-2: "a" pays 5%, "b" 3%. From day 3: "b" pays 12%.
        let history = days(7, |i| {
            let b_apy = if i < 3 { "3" } else { "12" };
            vec![quote("a", "5"), quote("b", b_apy)]
        });

        let points = run(&history, d("10000"), Risk::High, sync(mock_decide)).unwrap();

        // Into "a" on day 0 (one entry), then a to b on day 3 (exit + entry).
        let after_a = grown(invested(), "5", 3) - gas() - gas();
        let expected = grown(after_a, "12", 4);
        assert!(
            within_a_cent(points[6].equity_usd, expected),
            "{}",
            points[6].equity_usd
        );
    }

    #[test]
    fn holding_cash_keeps_equity_flat() {
        let history = days(5, |_| vec![quote("a", "10")]);

        let points = run(&history, d("10000"), Risk::High, sync(|_| hold())).unwrap();

        assert!(points.iter().all(|p| p.equity_usd == d("10000")));
    }

    #[test]
    fn rebalance_into_a_village_without_data_today_becomes_hold() {
        let history = days(3, |_| vec![quote("a", "10")]);
        let into_ghost = |_: &MarketState| Action::Rebalance {
            weights: BTreeMap::from([("ghost".to_string(), 100)]),
            reason: "test".to_string(),
        };

        let points = run(&history, d("10000"), Risk::High, sync(into_ghost)).unwrap();

        assert!(points.iter().all(|p| p.equity_usd == d("10000")));
    }

    #[test]
    fn rebalance_with_bad_weights_becomes_hold() {
        let history = days(3, |_| vec![quote("a", "10")]);
        let bad = |_: &MarketState| Action::Rebalance {
            weights: BTreeMap::from([("a".to_string(), 50)]),
            reason: "test".to_string(),
        };

        let points = run(&history, d("10000"), Risk::High, sync(bad)).unwrap();

        assert!(points.iter().all(|p| p.equity_usd == d("10000")));
    }

    #[test]
    fn move_from_cash_is_refused() {
        // D27: cash is invested with REBALANCE only.
        let history = days(3, |_| vec![quote("a", "10")]);
        let mover = |_: &MarketState| Action::Move {
            from: "cash".to_string(),
            to: "a".to_string(),
            percent: 100,
            reason: "test".to_string(),
        };

        let points = run(&history, d("10000"), Risk::High, sync(mover)).unwrap();

        assert!(points.iter().all(|p| p.equity_usd == d("10000")));
    }

    #[test]
    fn move_between_villages_is_applied() {
        // Day 0: all into "a" (5%). Day 1: move everything to "b" (10%).
        let history = days(3, |_| vec![quote("a", "5"), quote("b", "10")]);
        let plan = |state: &MarketState| {
            if state.positions.is_empty() {
                Action::Rebalance {
                    weights: BTreeMap::from([("a".to_string(), 100)]),
                    reason: "test".to_string(),
                }
            } else {
                Action::Move {
                    from: "a".to_string(),
                    to: "b".to_string(),
                    percent: 100,
                    reason: "test".to_string(),
                }
            }
        };

        let points = run(&history, d("10000"), Risk::High, sync(plan)).unwrap();

        let day_0 = grown(invested(), "5", 1);
        assert!(within_a_cent(points[0].equity_usd, day_0));
        // Day 1 moves a to b (exit + entry gas), then 2 days at 10%.
        assert!(within_a_cent(
            points[2].equity_usd,
            grown(day_0 - gas() - gas(), "10", 2)
        ));
    }

    #[test]
    fn cooldown_blocks_moves_on_the_following_days() {
        // Medium risk: 3 ticks (days) between moves. Every day the rule asks
        // to shift 5 more points from "a" (5%) to "b" (10%): a real gain, so
        // only the cooldown can refuse it. Only days 0 and 3 may move.
        let history = days(5, |_| vec![quote("a", "5"), quote("b", "10")]);
        let mut a_share_each_morning: Vec<Option<Decimal>> = Vec::new();
        let mut day = 0u8;
        let shift = |state: &MarketState| {
            let equity = state.cash_usd + state.positions.values().copied().sum::<Decimal>();
            let a_share = state
                .positions
                .get("a")
                .map(|a| (*a * Decimal::ONE_HUNDRED / equity).round());
            a_share_each_morning.push(a_share);
            let a_weight = 60 - 5 * day;
            day += 1;
            Action::Rebalance {
                weights: BTreeMap::from([
                    ("a".to_string(), a_weight),
                    ("b".to_string(), 100 - a_weight),
                ]),
                reason: "test".to_string(),
            }
        };

        run(&history, d("10000"), Risk::Medium, sync(shift)).unwrap();

        // Day 0 invests 60/40; days 1-2 are refused; day 3 moves to 45/55.
        let expected = [None, Some(60), Some(60), Some(60), Some(45)];
        assert_eq!(a_share_each_morning, expected.map(|s| s.map(Decimal::from)));
    }

    #[test]
    fn low_risk_refuses_all_in_one_village() {
        // Max share 60% for low risk: the mock's 100% rebalance is refused.
        let history = days(3, |_| vec![quote("a", "10")]);

        let points = run(&history, d("10000"), Risk::Low, sync(mock_decide)).unwrap();

        assert!(points.iter().all(|p| p.equity_usd == d("10000")));
    }

    #[test]
    fn a_day_without_data_still_has_a_point_and_keeps_growing() {
        // Day 1 has no data at all; money placed on day 0 keeps its 10% (D23).
        let history = days(3, |i| {
            if i == 1 {
                vec![]
            } else {
                vec![quote("a", "10")]
            }
        });

        let points = run(&history, d("10000"), Risk::High, sync(mock_decide)).unwrap();

        assert_eq!(points.len(), 3);
        assert!(within_a_cent(
            points[2].equity_usd,
            grown(invested(), "10", 3)
        ));
    }

    #[test]
    fn decide_sees_the_ledger_as_it_is_that_morning() {
        let history = days(2, |_| vec![quote("a", "10")]);
        let mut seen_cash = Vec::new();

        run(
            &history,
            d("10000"),
            Risk::High,
            sync(|state| {
                seen_cash.push(state.cash_usd);
                mock_decide(state)
            }),
        )
        .unwrap();

        // Day 0: all cash. Day 1: everything was moved to "a" the day before.
        assert_eq!(seen_cash, vec![d("10000"), Decimal::ZERO]);
    }

    #[test]
    fn decide_is_told_the_days_since_the_last_move() {
        // Day 0 invests (a move), days 1-3 hold: the LLM prompt shows the
        // cooldown from this number, so it must count up from the move.
        let history = days(4, |_| vec![quote("a", "10")]);
        let mut seen_ticks = Vec::new();

        run(
            &history,
            d("10000"),
            Risk::High,
            |state: &MarketState, ticks: Option<u32>| {
                seen_ticks.push(ticks);
                mock_decide(state)
            },
        )
        .unwrap();

        assert_eq!(seen_ticks, vec![None, Some(1), Some(2), Some(3)]);
    }

    // ---- database tests (run_for_avatar) ----

    /// "Now" for the DB tests: 4 Nov 2026 at 15:00 UTC, so the window is the
    /// 7 complete days 28 Oct .. 3 Nov.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 4, 15, 0, 0).unwrap()
    }

    /// The persona of the DB test avatar; the tests look for it in the prompt.
    const PERSONA: &str = "Prefer lending, never touch DEX pools.";
    pub(crate) const HOLD_ANSWER: &str =
        r#"{"action":"HOLD","reason":"Waiting for a better rate."}"#;
    const ALL_IN_USDC: &str =
        r#"{"action":"REBALANCE","weights":{"rhea-lend-usdc":100},"reason":"Best lending rate."}"#;

    /// A fake OpenAI-compatible server that always answers `content` and
    /// keeps every request body, so tests can count calls and read prompts.
    pub(crate) struct FakeLlm {
        pub(crate) client: LlmClient,
        pub(crate) requests: Arc<Mutex<Vec<Value>>>,
    }

    impl FakeLlm {
        pub(crate) fn calls(&self) -> usize {
            self.requests.lock().unwrap().len()
        }
    }

    pub(crate) async fn fake_llm(content: &str) -> FakeLlm {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route(
                "/chat/completions",
                post(
                    |State((content, requests)): State<(String, Arc<Mutex<Vec<Value>>>)>,
                     Json(body): Json<Value>| async move {
                        requests.lock().unwrap().push(body);
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
            .with_state((content.to_string(), requests.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        FakeLlm {
            client: client(&format!("http://{addr}")),
            requests,
        }
    }

    fn client(base_url: &str) -> LlmClient {
        LlmClient::new(&LlmConfig {
            base_url: base_url.to_string(),
            api_key: "test-key".to_string(),
            model: "fake-model".to_string(),
        })
    }

    async fn create_avatar(pool: &PgPool) -> Uuid {
        let owner = "alice.testnet";
        avatars::ensure_user(pool, owner).await.unwrap();
        let config_id = avatars::insert_config(
            pool,
            &avatars::NewConfig {
                persona: PERSONA.to_string(),
                // High risk: all-in on one village is allowed (low and medium
                // risk refuse it: max share 60% / 80%).
                risk: "high".to_string(),
                goal: "steady_income".to_string(),
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
                starting_capital_usd: Decimal::from(10_000),
                active: true,
            },
        )
        .await
        .unwrap()
    }

    /// One snapshot per day at 23:00 UTC for the `count` days before `now()`'s
    /// date, plus one point on `now()`'s own (incomplete) day.
    async fn seed_history(pool: &PgPool, village_id: &str, apy: &str, count: u64) {
        let today = now().date_naive();
        for back in 0..=count {
            let day = today - Days::new(back);
            let snapshot = yield_snapshots::YieldSnapshot {
                village_id: village_id.to_string(),
                ts: day.and_hms_opt(23, 0, 0).unwrap().and_utc().min(now()),
                apy_base: d(apy),
                apy_reward: Decimal::ZERO,
                tvl_usd: d("1000000"),
            };
            yield_snapshots::insert_snapshot(pool, &snapshot)
                .await
                .unwrap();
        }
    }

    async fn backtest_decisions(pool: &PgPool, avatar: Uuid) -> Vec<decisions::DecisionRow> {
        let mut rows = decisions::list_decisions(pool, avatar, SOURCE, 100)
            .await
            .unwrap();
        rows.reverse(); // oldest first, like the equity points
        rows
    }

    /// Day 2 acceptance (HANDOFF §4, D60) with the LLM (D63): 7 equity rows
    /// and 7 decisions, all labeled backtest, one LLM call per day.
    #[sqlx::test]
    async fn backtest_stores_seven_days_of_equity_and_decisions(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        seed_history(&pool, "rhea-lend-usdc", "14", 40).await;
        seed_history(&pool, "rhea-lend-usdt", "12", 40).await;
        let llm = fake_llm(ALL_IN_USDC).await;

        run_for_avatar(&pool, &llm.client, avatar, now())
            .await
            .unwrap();

        let stored = equity::list_equity(&pool, avatar, SOURCE).await.unwrap();
        // Follows the constant, so changing the window needs no test edit.
        assert_eq!(stored.len(), BACKTEST_DAYS as usize);
        assert_eq!(stored.len(), 7);
        assert_eq!(
            stored[0].ts,
            Utc.with_ymd_and_hms(2026, 10, 29, 0, 0, 0).unwrap()
        );
        assert_eq!(
            stored[6].ts,
            Utc.with_ymd_and_hms(2026, 11, 4, 0, 0, 0).unwrap()
        );
        // Every day's state differs (cash, then positions), so no cache hit.
        assert_eq!(llm.calls(), 7);

        let rows = backtest_decisions(&pool, avatar).await;
        assert_eq!(rows.len(), 7);
        // Decisions are stamped at the start of their day (equity at the end).
        assert_eq!(
            rows[0].ts,
            Utc.with_ymd_and_hms(2026, 10, 28, 0, 0, 0).unwrap()
        );
        // Day 1 follows the model: all into the 14% village.
        assert!(rows[0].valid, "{:?}", rows[0].rejected_rule);
        assert_eq!(rows[0].applied_action["action"], "REBALANCE");
        assert_eq!(rows[0].model, "fake-model");
        assert!(stored[6].equity_usd > d("10000"));
        // Nothing is written under another label.
        assert!(
            equity::list_equity(&pool, avatar, "sim")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            decisions::list_decisions(&pool, avatar, "sim", 100)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[sqlx::test]
    async fn the_persona_is_sent_to_the_model(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        seed_history(&pool, "rhea-lend-usdc", "14", 40).await;
        let llm = fake_llm(HOLD_ANSWER).await;

        run_for_avatar(&pool, &llm.client, avatar, now())
            .await
            .unwrap();

        // The whole point of D63: the backtest tests *this* strategy.
        let requests = llm.requests.lock().unwrap();
        let system = requests[0]["messages"][0]["content"].as_str().unwrap();
        assert!(system.contains(PERSONA), "persona missing from the prompt");
    }

    #[sqlx::test]
    async fn the_public_reasons_never_contain_the_persona(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        seed_history(&pool, "rhea-lend-usdc", "14", 40).await;
        let llm = fake_llm(HOLD_ANSWER).await;

        run_for_avatar(&pool, &llm.client, avatar, now())
            .await
            .unwrap();

        for row in backtest_decisions(&pool, avatar).await {
            let reason = row.reason_public.unwrap_or_default();
            assert!(!reason.contains(PERSONA), "{reason}");
        }
    }

    #[sqlx::test]
    async fn an_unreachable_llm_gives_seven_holds_and_flat_equity(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        seed_history(&pool, "rhea-lend-usdc", "14", 40).await;
        // Port 9 (discard) on localhost: nothing listens, connection refused.
        let llm = client("http://127.0.0.1:9");

        let points = run_for_avatar(&pool, &llm, avatar, now()).await.unwrap();

        // The backtest still completes: each day is HOLD, money stays cash.
        assert!(points.iter().all(|p| p.equity_usd == d("10000")));
        let rows = backtest_decisions(&pool, avatar).await;
        assert_eq!(rows.len(), 7);
        assert!(
            rows.iter()
                .all(|row| row.rejected_rule.as_deref() == Some("llm_error"))
        );
    }

    #[sqlx::test]
    async fn running_the_backtest_again_replaces_the_rows_and_uses_the_cache(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        seed_history(&pool, "rhea-lend-usdc", "14", 40).await;
        let llm = fake_llm(HOLD_ANSWER).await;

        run_for_avatar(&pool, &llm.client, avatar, now())
            .await
            .unwrap();
        run_for_avatar(&pool, &llm.client, avatar, now())
            .await
            .unwrap();

        let stored = equity::list_equity(&pool, avatar, SOURCE).await.unwrap();
        assert_eq!(stored.len(), 7);
        assert_eq!(backtest_decisions(&pool, avatar).await.len(), 7);
        // Same persona, same past days: the second run is answered by
        // `llm_cache` and costs no new call.
        assert_eq!(llm.calls(), 7);
    }

    #[sqlx::test]
    async fn short_history_gives_a_shorter_backtest(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        seed_history(&pool, "rhea-lend-usdc", "14", 3).await;
        let llm = fake_llm(HOLD_ANSWER).await;

        let points = run_for_avatar(&pool, &llm.client, avatar, now())
            .await
            .unwrap();

        assert_eq!(points.len(), 3);
    }

    #[sqlx::test]
    async fn no_history_is_an_error(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        let llm = fake_llm(HOLD_ANSWER).await;

        let err = run_for_avatar(&pool, &llm.client, avatar, now())
            .await
            .unwrap_err();

        assert!(err.to_string().contains("no yield history"), "{err}");
        assert_eq!(llm.calls(), 0);
    }

    #[sqlx::test]
    async fn unknown_avatar_is_an_error(pool: PgPool) {
        let llm = fake_llm(HOLD_ANSWER).await;

        let err = run_for_avatar(&pool, &llm.client, Uuid::nil(), now())
            .await
            .unwrap_err();

        assert!(err.to_string().contains("not found"), "{err}");
    }
}
