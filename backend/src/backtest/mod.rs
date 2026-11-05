//! Backtest: replay the last 30 days of real yield history with an avatar's
//! decision rule and store the resulting equity curve, labeled `backtest`
//! (SPEC F5, TECH §5.2).
//!
//! Two layers:
//! - `run` is pure (history in, equity points out), so the replay rules are
//!   unit-tested without a database.
//! - `run_for_avatar` loads the avatar and history from Postgres, calls `run`
//!   and saves the result in one transaction (D24).
//!
//! Each day: build the `MarketState`, ask `decide`, check the action with the
//! guardrails (a refused action becomes HOLD), grow positions by one day of
//! yield, record equity. One day = one tick for the cooldown rule. Costs come
//! with the Day 4 cost model.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Days, NaiveDate, NaiveTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::agent::guardrails::{self, Risk};
use crate::agent::{Action, MarketState, VillageQuote, mock_decide};
use crate::db::equity::{self, EquityPoint};
use crate::db::{avatars, villages, yield_snapshots};
use crate::sim::ledger::{Ledger, LedgerError};

/// Length of the backtest window (SPEC F5.1, D5).
pub const BACKTEST_DAYS: u64 = 30;

/// The villages that have data on one UTC day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayQuotes {
    pub day: NaiveDate,
    /// Empty when no village has data that day.
    pub villages: Vec<VillageQuote>,
}

/// Replays `days` in order, starting with `starting_capital` in cash, and
/// returns one equity point per day, timestamped at the **end** of that day
/// (next UTC midnight).
///
/// Every action goes through `guardrails::check` for `risk` ("the LLM
/// proposes, code decides"). A refused action becomes HOLD and is logged with
/// the rule. A village without data that day is not in `villages`, so it
/// cannot receive money (D23).
///
/// # Errors
/// `LedgerError::Overflow` if an amount no longer fits a `Decimal`.
pub fn run(
    days: &[DayQuotes],
    starting_capital: Decimal,
    risk: Risk,
    mut decide: impl FnMut(&MarketState) -> Action,
) -> Result<Vec<EquityPoint>, LedgerError> {
    let mut ledger = Ledger::new(starting_capital);
    let mut points = Vec::with_capacity(days.len());
    // Index of the day of the last applied MOVE or REBALANCE (cooldown).
    let mut last_move_day: Option<usize> = None;

    for (day_index, today) in days.iter().enumerate() {
        let state = MarketState {
            day: today.day,
            villages: today.villages.clone(),
            cash_usd: ledger.cash_usd(),
            positions: ledger.positions().clone(),
        };
        let action = decide(&state);
        let ticks_since_last_move = last_move_day.map(|day| (day_index - day) as u32);
        if apply_action(&mut ledger, &action, &state, risk, ticks_since_last_move) {
            last_move_day = Some(day_index);
        }

        let apys: BTreeMap<String, Decimal> = today
            .villages
            .iter()
            .map(|quote| (quote.village_id.clone(), quote.apy))
            .collect();
        ledger.accrue_one_day(&apys)?;

        points.push(EquityPoint {
            ts: end_of_day(today.day),
            equity_usd: ledger.equity(),
        });
    }
    Ok(points)
}

/// Applies `action` to the ledger if the guardrails allow it; otherwise the
/// ledger is unchanged (= HOLD) and the refusal is logged with the rule.
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
    if let Err(rule) = guardrails::check(action, state, risk, ticks_since_last_move) {
        tracing::warn!(day = %state.day, %rule, "backtest: action refused by guardrails, holding");
        return false;
    }
    let result = match action {
        // SAY never changes positions (SPEC §5).
        Action::Hold { .. } | Action::Say { .. } => return false,
        Action::Move {
            from, to, percent, ..
        } => ledger.move_percent(from, to, *percent),
        Action::Rebalance { weights, .. } => ledger.rebalance(weights),
    };
    match result {
        Ok(()) => true,
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
/// UTC days before `now`, and replaces its stored backtest equity (D24).
///
/// Uses the mock decision rule until the LLM arrives on Day 3. Starts from the
/// avatar's `starting_capital_usd` (the backtest is a "what if you had started
/// 30 days ago", not its current cash). `now` is a parameter so tests are
/// deterministic.
///
/// The replay starts at the first day that has data for any village, and then
/// covers every calendar day up to yesterday: a day without any data still
/// gets an equity point (money grows at the last known APY, D23). With less
/// than 30 days of history it runs the days it has and logs a warning.
///
/// Returns the stored points.
///
/// # Errors
/// Unknown avatar, no history at all in the window, a database error, or a
/// ledger overflow.
pub async fn run_for_avatar(
    pool: &PgPool,
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
    let points = run(&days, avatar.starting_capital_usd, risk, mock_decide)?;
    equity::replace_backtest_equity(pool, avatar_id, &points).await?;

    tracing::info!(
        avatar = %avatar_id,
        days = points.len(),
        final_equity = %points.last().map(|p| p.equity_usd).unwrap_or_default(),
        "backtest stored"
    );
    Ok(points)
}

/// Loads the daily history and turns it into one `DayQuotes` per calendar day,
/// from the first day with any data up to the day before `window_end`.
async fn load_days(
    pool: &PgPool,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Result<Vec<DayQuotes>> {
    let kinds: BTreeMap<String, String> = villages::list_active_villages(pool)
        .await?
        .into_iter()
        .map(|village| (village.id, village.kind))
        .collect();

    let mut by_day: BTreeMap<NaiveDate, Vec<VillageQuote>> = BTreeMap::new();
    for snapshot in yield_snapshots::daily_history(pool, window_start, window_end).await? {
        // `daily_history` only returns active villages, so the kind is known;
        // skip defensively if a village was deactivated between the two queries.
        let Some(kind) = kinds.get(&snapshot.village_id) else {
            continue;
        };
        by_day
            .entry(snapshot.ts.date_naive())
            .or_default()
            .push(VillageQuote {
                village_id: snapshot.village_id,
                kind: kind.clone(),
                apy: snapshot.apy_base + snapshot.apy_reward,
                tvl_usd: snapshot.tvl_usd,
            });
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
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rust_decimal::MathematicalOps;

    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    fn quote(village_id: &str, apy: &str) -> VillageQuote {
        VillageQuote {
            village_id: village_id.to_string(),
            kind: "lending".to_string(),
            apy: d(apy),
            tvl_usd: d("1000000"),
        }
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
    fn grown(value: &str, apy: &str, days: i64) -> Decimal {
        let daily =
            (Decimal::ONE + d(apy) / Decimal::ONE_HUNDRED).powd(Decimal::ONE / Decimal::from(365));
        d(value) * daily.powi(days)
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
    fn thirty_days_give_thirty_points_stamped_at_end_of_day() {
        let history = days(30, |_| vec![quote("a", "10")]);

        let points = run(&history, d("10000"), Risk::High, mock_decide).unwrap();

        assert_eq!(points.len(), 30);
        assert_eq!(
            points[0].ts,
            Utc.with_ymd_and_hms(2026, 10, 2, 0, 0, 0).unwrap()
        );
        assert_eq!(
            points[29].ts,
            Utc.with_ymd_and_hms(2026, 10, 31, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn flat_apy_grows_equity_every_day_by_the_formula() {
        let history = days(30, |_| vec![quote("a", "10")]);

        let points = run(&history, d("10000"), Risk::High, mock_decide).unwrap();

        assert!(
            points
                .windows(2)
                .all(|pair| pair[1].equity_usd > pair[0].equity_usd)
        );
        assert!(within_a_cent(
            points[29].equity_usd,
            grown("10000", "10", 30)
        ));
    }

    #[test]
    fn mock_follows_the_best_village_when_it_changes() {
        // Days 0-8: "a" pays 5%, "b" 3%. From day 9: "b" pays 12%.
        let history = days(30, |i| {
            let b_apy = if i < 9 { "3" } else { "12" };
            vec![quote("a", "5"), quote("b", b_apy)]
        });

        let points = run(&history, d("10000"), Risk::High, mock_decide).unwrap();

        let after_a = grown("10000", "5", 9);
        let expected = after_a * grown("1", "12", 21);
        assert!(
            within_a_cent(points[29].equity_usd, expected),
            "{}",
            points[29].equity_usd
        );
    }

    #[test]
    fn holding_cash_keeps_equity_flat() {
        let history = days(5, |_| vec![quote("a", "10")]);

        let points = run(&history, d("10000"), Risk::High, |_| hold()).unwrap();

        assert!(points.iter().all(|p| p.equity_usd == d("10000")));
    }

    #[test]
    fn rebalance_into_a_village_without_data_today_becomes_hold() {
        let history = days(3, |_| vec![quote("a", "10")]);
        let into_ghost = |_: &MarketState| Action::Rebalance {
            weights: BTreeMap::from([("ghost".to_string(), 100)]),
            reason: "test".to_string(),
        };

        let points = run(&history, d("10000"), Risk::High, into_ghost).unwrap();

        assert!(points.iter().all(|p| p.equity_usd == d("10000")));
    }

    #[test]
    fn rebalance_with_bad_weights_becomes_hold() {
        let history = days(3, |_| vec![quote("a", "10")]);
        let bad = |_: &MarketState| Action::Rebalance {
            weights: BTreeMap::from([("a".to_string(), 50)]),
            reason: "test".to_string(),
        };

        let points = run(&history, d("10000"), Risk::High, bad).unwrap();

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

        let points = run(&history, d("10000"), Risk::High, mover).unwrap();

        assert!(points.iter().all(|p| p.equity_usd == d("10000")));
    }

    #[test]
    fn move_between_villages_is_applied() {
        // Day 0: all into "a" (0%). Day 1: move everything to "b" (10%).
        let history = days(3, |_| vec![quote("a", "0"), quote("b", "10")]);
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

        let points = run(&history, d("10000"), Risk::High, plan).unwrap();

        assert_eq!(points[0].equity_usd, d("10000")); // day 0 in "a" at 0%
        assert!(within_a_cent(points[2].equity_usd, grown("10000", "10", 2)));
    }

    #[test]
    fn cooldown_blocks_moves_on_the_following_days() {
        // Medium risk: 3 ticks (days) between moves. The rule asks to flip
        // between 60/40 and 40/60 every day (20% moved, under the 50% max
        // move); only days 0 and 3 may move.
        let history = days(5, |_| vec![quote("a", "10"), quote("b", "10")]);
        let mut biggest_each_morning: Vec<Option<String>> = Vec::new();
        let flip = |state: &MarketState| {
            let biggest = state
                .positions
                .iter()
                .max_by_key(|(_, value)| **value)
                .map(|(id, _)| id.clone());
            let into = if biggest.as_deref() == Some("a") {
                "b"
            } else {
                "a"
            };
            let other = if into == "a" { "b" } else { "a" };
            biggest_each_morning.push(biggest);
            Action::Rebalance {
                weights: BTreeMap::from([(into.to_string(), 60), (other.to_string(), 40)]),
                reason: "test".to_string(),
            }
        };

        run(&history, d("10000"), Risk::Medium, flip).unwrap();

        let expected = [None, Some("a"), Some("a"), Some("a"), Some("b")];
        assert_eq!(
            biggest_each_morning,
            expected.map(|id| id.map(str::to_string))
        );
    }

    #[test]
    fn low_risk_refuses_all_in_one_village() {
        // Max share 60% for low risk: the mock's 100% rebalance is refused.
        let history = days(3, |_| vec![quote("a", "10")]);

        let points = run(&history, d("10000"), Risk::Low, mock_decide).unwrap();

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

        let points = run(&history, d("10000"), Risk::High, mock_decide).unwrap();

        assert_eq!(points.len(), 3);
        assert!(within_a_cent(points[2].equity_usd, grown("10000", "10", 3)));
    }

    #[test]
    fn decide_sees_the_ledger_as_it_is_that_morning() {
        let history = days(2, |_| vec![quote("a", "10")]);
        let mut seen_cash = Vec::new();

        run(&history, d("10000"), Risk::High, |state| {
            seen_cash.push(state.cash_usd);
            mock_decide(state)
        })
        .unwrap();

        // Day 0: all cash. Day 1: everything was moved to "a" the day before.
        assert_eq!(seen_cash, vec![d("10000"), Decimal::ZERO]);
    }

    // ---- database tests (run_for_avatar) ----

    /// "Now" for the DB tests: 4 Nov 2026 at 15:00 UTC, so the window is the
    /// 30 complete days 5 Oct .. 3 Nov.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 4, 15, 0, 0).unwrap()
    }

    async fn create_avatar(pool: &PgPool) -> Uuid {
        let owner = "alice.testnet";
        avatars::ensure_user(pool, owner).await.unwrap();
        let config_id = avatars::insert_config(
            pool,
            &avatars::NewConfig {
                persona: "Prefer lending.".to_string(),
                // High risk: the mock goes all-in on one village, which low and
                // medium risk refuse (max share 60% / 80%).
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

    /// Day 2 acceptance (HANDOFF §4): a test avatar ends with 30 backtest rows.
    #[sqlx::test]
    async fn backtest_stores_thirty_equity_rows_for_a_test_avatar(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        seed_history(&pool, "rhea-lend-usdc", "14", 40).await;
        seed_history(&pool, "rhea-lend-usdt", "12", 40).await;

        run_for_avatar(&pool, avatar, now()).await.unwrap();

        let stored = equity::list_equity(&pool, avatar, "backtest")
            .await
            .unwrap();
        assert_eq!(stored.len(), 30);
        assert_eq!(
            stored[0].ts,
            Utc.with_ymd_and_hms(2026, 10, 6, 0, 0, 0).unwrap()
        );
        assert_eq!(
            stored[29].ts,
            Utc.with_ymd_and_hms(2026, 11, 4, 0, 0, 0).unwrap()
        );
        // Mock picks the 14% village on day 1 and stays there.
        assert!(within_a_cent(
            stored[29].equity_usd,
            grown("10000", "14", 30)
        ));
        // Nothing is written under another label.
        assert!(
            equity::list_equity(&pool, avatar, "sim")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[sqlx::test]
    async fn running_the_backtest_again_replaces_the_rows(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        seed_history(&pool, "rhea-lend-usdc", "14", 40).await;

        run_for_avatar(&pool, avatar, now()).await.unwrap();
        run_for_avatar(&pool, avatar, now()).await.unwrap();

        let stored = equity::list_equity(&pool, avatar, "backtest")
            .await
            .unwrap();
        assert_eq!(stored.len(), 30);
    }

    #[sqlx::test]
    async fn short_history_gives_a_shorter_backtest(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        seed_history(&pool, "rhea-lend-usdc", "14", 10).await;

        let points = run_for_avatar(&pool, avatar, now()).await.unwrap();

        assert_eq!(points.len(), 10);
    }

    #[sqlx::test]
    async fn no_history_is_an_error(pool: PgPool) {
        let avatar = create_avatar(&pool).await;

        let err = run_for_avatar(&pool, avatar, now()).await.unwrap_err();

        assert!(err.to_string().contains("no yield history"), "{err}");
    }

    #[sqlx::test]
    async fn unknown_avatar_is_an_error(pool: PgPool) {
        let err = run_for_avatar(&pool, Uuid::nil(), now()).await.unwrap_err();

        assert!(err.to_string().contains("not found"), "{err}");
    }
}
