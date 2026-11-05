//! Guardrails: the risk rules every proposed action must pass (SPEC §6).
//!
//! "The LLM proposes, code decides." `check` is a pure function: it looks at
//! the action, what the avatar holds and the villages it may use, and says
//! yes or which rule says no. A rejected action becomes HOLD in the caller,
//! and the rule name is stored in `decisions.rejected_rule`.
//!
//! Not here yet (HANDOFF D28): the two cost-based rules ("min APR gain after
//! costs" and "cost larger than 7 days of gain"). They need the cost model,
//! which arrives on Day 4.

use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;

use super::{Action, MarketState};

/// Max length of the public `reason` (SPEC §5, F4.3), in characters.
pub const MAX_REASON_CHARS: usize = 140;

/// The avatar's risk level, chosen at creation and stored in
/// `avatar_configs.risk`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    Low,
    Medium,
    High,
}

impl FromStr for Risk {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            other => Err(format!("unknown risk level '{other}'")),
        }
    }
}

/// The hard limits of one risk level (SPEC §6 table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiskLimits {
    /// Village kinds this risk level may put money into.
    pub allowed_kinds: &'static [&'static str],
    /// Max share of equity taken out of villages in one tick, in percent.
    pub max_percent_moved: u8,
    /// Ticks that must pass after a move before the next one.
    pub cooldown_ticks: u32,
    /// Max share of equity one village may hold after the action, in percent.
    pub max_share_percent: u8,
}

impl Risk {
    pub fn limits(self) -> RiskLimits {
        match self {
            // "Stablecoin lending only": all our lending villages are stablecoin.
            Self::Low => RiskLimits {
                allowed_kinds: &["lending"],
                max_percent_moved: 25,
                cooldown_ticks: 6,
                max_share_percent: 60,
            },
            Self::Medium => RiskLimits {
                allowed_kinds: &["lending", "stable_lp"],
                max_percent_moved: 50,
                cooldown_ticks: 3,
                max_share_percent: 80,
            },
            // "All": today that is the same two kinds; a new kind (e.g.
            // volatile LP) would be added here only.
            Self::High => RiskLimits {
                allowed_kinds: &["lending", "stable_lp"],
                max_percent_moved: 100,
                cooldown_ticks: 1,
                max_share_percent: 100,
            },
        }
    }
}

/// The rule that refused an action. `as_str` is the value stored in
/// `decisions.rejected_rule`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectedRule {
    /// `reason` longer than `MAX_REASON_CHARS`.
    ReasonTooLong,
    /// MOVE `percent` outside 1..=100.
    InvalidPercent,
    /// REBALANCE weights do not add up to 100.
    WeightsNot100,
    /// MOVE with the same village as source and target.
    SameVillage,
    /// MOVE from a village where the avatar holds nothing.
    NoPosition,
    /// Target village is unknown, stale, or has no data today.
    VillageUnavailable,
    /// Target village kind is not allowed for this risk level.
    KindNotAllowed,
    /// Too much money taken out of villages in one tick.
    MaxMove,
    /// Too soon after the last move.
    Cooldown,
    /// One village would hold more than the max share.
    MaxShare,
}

impl RejectedRule {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReasonTooLong => "reason_too_long",
            Self::InvalidPercent => "invalid_percent",
            Self::WeightsNot100 => "weights_not_100",
            Self::SameVillage => "same_village",
            Self::NoPosition => "no_position",
            Self::VillageUnavailable => "village_unavailable",
            Self::KindNotAllowed => "kind_not_allowed",
            Self::MaxMove => "max_move",
            Self::Cooldown => "cooldown",
            Self::MaxShare => "max_share",
        }
    }
}

impl fmt::Display for RejectedRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Checks `action` against the guardrails for `risk`.
///
/// `ticks_since_last_move` is `None` when the avatar never moved money.
///
/// `state.villages` must list only villages that may receive funds this tick
/// (fresh data, not stale). Money may still be taken *out* of a village that
/// is missing from the list.
///
/// HOLD and SAY only need a valid reason: they never move money.
///
/// # Errors
/// The first rule that fails, in this order: field limits, villages, cooldown,
/// max move, max share.
pub fn check(
    action: &Action,
    state: &MarketState,
    risk: Risk,
    ticks_since_last_move: Option<u32>,
) -> Result<(), RejectedRule> {
    let limits = risk.limits();
    check_reason(action)?;

    match action {
        Action::Hold { .. } | Action::Say { .. } => Ok(()),
        Action::Move {
            from, to, percent, ..
        } => {
            if !(1..=100).contains(percent) {
                return Err(RejectedRule::InvalidPercent);
            }
            if from == to {
                return Err(RejectedRule::SameVillage);
            }
            let source_value = state.positions.get(from).copied().unwrap_or_default();
            if source_value <= Decimal::ZERO {
                return Err(RejectedRule::NoPosition);
            }
            check_target(to, state, &limits)?;
            check_cooldown(ticks_since_last_move, &limits)?;

            let moved = source_value * Decimal::from(*percent) / Decimal::ONE_HUNDRED;
            let equity = equity(state);
            check_max_move(moved, equity, &limits)?;

            let target_after = state.positions.get(to).copied().unwrap_or_default() + moved;
            check_max_share(target_after, equity, &limits)
        }
        Action::Rebalance { weights, .. } => {
            let sum: u32 = weights.values().map(|&w| u32::from(w)).sum();
            if sum != 100 {
                return Err(RejectedRule::WeightsNot100);
            }
            // Zero weights mean "nothing here" (the ledger drops them).
            for (village_id, _) in weights.iter().filter(|(_, w)| **w > 0) {
                check_target(village_id, state, &limits)?;
            }
            check_cooldown(ticks_since_last_move, &limits)?;

            let equity = equity(state);
            let target = |village_id: &str| {
                let weight = weights.get(village_id).copied().unwrap_or(0);
                equity * Decimal::from(weight) / Decimal::ONE_HUNDRED
            };
            // Money taken out of villages. Cash put to work is not counted
            // (HANDOFF D29): a new avatar must be able to invest its cash.
            let moved: Decimal = state
                .positions
                .iter()
                .map(|(village_id, value)| (*value - target(village_id)).max(Decimal::ZERO))
                .sum();
            check_max_move(moved, equity, &limits)?;

            for weight in weights.values() {
                if *weight > limits.max_share_percent {
                    return Err(RejectedRule::MaxShare);
                }
            }
            Ok(())
        }
    }
}

fn check_reason(action: &Action) -> Result<(), RejectedRule> {
    let reason = match action {
        Action::Hold { reason }
        | Action::Move { reason, .. }
        | Action::Rebalance { reason, .. }
        | Action::Say { reason, .. } => reason,
    };
    if reason.chars().count() > MAX_REASON_CHARS {
        return Err(RejectedRule::ReasonTooLong);
    }
    Ok(())
}

/// A village may receive money only if it is listed this tick (fresh data)
/// and its kind is allowed for the risk level.
fn check_target(
    village_id: &str,
    state: &MarketState,
    limits: &RiskLimits,
) -> Result<(), RejectedRule> {
    let quote = state
        .villages
        .iter()
        .find(|quote| quote.village_id == village_id)
        .ok_or(RejectedRule::VillageUnavailable)?;
    if !limits.allowed_kinds.contains(&quote.kind.as_str()) {
        return Err(RejectedRule::KindNotAllowed);
    }
    Ok(())
}

fn check_cooldown(
    ticks_since_last_move: Option<u32>,
    limits: &RiskLimits,
) -> Result<(), RejectedRule> {
    match ticks_since_last_move {
        Some(ticks) if ticks < limits.cooldown_ticks => Err(RejectedRule::Cooldown),
        _ => Ok(()),
    }
}

/// `moved / equity <= max%`, written as a multiplication so a zero equity
/// cannot divide by zero.
fn check_max_move(
    moved: Decimal,
    equity: Decimal,
    limits: &RiskLimits,
) -> Result<(), RejectedRule> {
    if moved * Decimal::ONE_HUNDRED > equity * Decimal::from(limits.max_percent_moved) {
        return Err(RejectedRule::MaxMove);
    }
    Ok(())
}

fn check_max_share(
    value: Decimal,
    equity: Decimal,
    limits: &RiskLimits,
) -> Result<(), RejectedRule> {
    if value * Decimal::ONE_HUNDRED > equity * Decimal::from(limits.max_share_percent) {
        return Err(RejectedRule::MaxShare);
    }
    Ok(())
}

fn equity(state: &MarketState) -> Decimal {
    state.cash_usd + state.positions.values().copied().sum::<Decimal>()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::NaiveDate;

    use super::*;
    use crate::agent::VillageQuote;

    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    fn quote(village_id: &str, kind: &str) -> VillageQuote {
        VillageQuote {
            village_id: village_id.to_string(),
            kind: kind.to_string(),
            apy: d("10"),
            tvl_usd: d("1000000"),
        }
    }

    /// Villages: two lending ("lend-a", "lend-b") and one stable LP ("lp").
    fn state(cash: &str, positions: &[(&str, &str)]) -> MarketState {
        MarketState {
            day: NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(),
            villages: vec![
                quote("lend-a", "lending"),
                quote("lend-b", "lending"),
                quote("lp", "stable_lp"),
            ],
            cash_usd: d(cash),
            positions: positions
                .iter()
                .map(|(id, value)| (id.to_string(), d(value)))
                .collect(),
        }
    }

    fn move_action(from: &str, to: &str, percent: u8) -> Action {
        Action::Move {
            from: from.to_string(),
            to: to.to_string(),
            percent,
            reason: "test".to_string(),
        }
    }

    fn rebalance(pairs: &[(&str, u8)]) -> Action {
        Action::Rebalance {
            weights: pairs
                .iter()
                .map(|(id, w)| (id.to_string(), *w))
                .collect::<BTreeMap<_, _>>(),
            reason: "test".to_string(),
        }
    }

    fn hold(reason: &str) -> Action {
        Action::Hold {
            reason: reason.to_string(),
        }
    }

    /// Long ago enough that no cooldown applies.
    const RESTED: Option<u32> = Some(100);

    #[test]
    fn risk_parses_from_the_database_values() {
        assert_eq!("low".parse::<Risk>(), Ok(Risk::Low));
        assert_eq!("medium".parse::<Risk>(), Ok(Risk::Medium));
        assert_eq!("high".parse::<Risk>(), Ok(Risk::High));
        assert!("LOW".parse::<Risk>().is_err());
    }

    #[test]
    fn hold_and_say_pass_even_during_cooldown() {
        let s = state("0", &[("lend-a", "100")]);
        let say = Action::Say {
            text: "hi".to_string(),
            reason: "test".to_string(),
        };

        assert_eq!(check(&hold("ok"), &s, Risk::Low, Some(0)), Ok(()));
        assert_eq!(check(&say, &s, Risk::Low, Some(0)), Ok(()));
    }

    #[test]
    fn reason_is_limited_to_140_characters_not_bytes() {
        let s = state("100", &[]);
        let exactly_140 = "é".repeat(140); // 280 bytes, 140 chars

        assert_eq!(check(&hold(&exactly_140), &s, Risk::High, None), Ok(()));
        assert_eq!(
            check(&hold(&"a".repeat(141)), &s, Risk::High, None),
            Err(RejectedRule::ReasonTooLong)
        );
    }

    #[test]
    fn move_percent_must_be_1_to_100() {
        let s = state("0", &[("lend-a", "100")]);

        assert_eq!(
            check(&move_action("lend-a", "lend-b", 0), &s, Risk::High, RESTED),
            Err(RejectedRule::InvalidPercent)
        );
        assert_eq!(
            check(
                &move_action("lend-a", "lend-b", 101),
                &s,
                Risk::High,
                RESTED
            ),
            Err(RejectedRule::InvalidPercent)
        );
        assert_eq!(
            check(
                &move_action("lend-a", "lend-b", 100),
                &s,
                Risk::High,
                RESTED
            ),
            Ok(())
        );
    }

    #[test]
    fn move_to_the_same_village_is_refused() {
        let s = state("0", &[("lend-a", "100")]);

        assert_eq!(
            check(&move_action("lend-a", "lend-a", 10), &s, Risk::High, RESTED),
            Err(RejectedRule::SameVillage)
        );
    }

    #[test]
    fn move_needs_money_in_the_source_village_and_cash_is_not_a_village() {
        let s = state("100", &[]);

        assert_eq!(
            check(&move_action("lend-a", "lend-b", 10), &s, Risk::High, RESTED),
            Err(RejectedRule::NoPosition)
        );
        // D27: cash is invested with REBALANCE, never with MOVE.
        assert_eq!(
            check(&move_action("cash", "lend-b", 10), &s, Risk::High, RESTED),
            Err(RejectedRule::NoPosition)
        );
    }

    #[test]
    fn money_cannot_go_to_a_stale_or_unknown_village() {
        let s = state("0", &[("lend-a", "100")]);

        assert_eq!(
            check(&move_action("lend-a", "ghost", 10), &s, Risk::High, RESTED),
            Err(RejectedRule::VillageUnavailable)
        );
        assert_eq!(
            check(
                &rebalance(&[("lend-a", 50), ("ghost", 50)]),
                &s,
                Risk::High,
                RESTED
            ),
            Err(RejectedRule::VillageUnavailable)
        );
    }

    #[test]
    fn money_can_leave_a_village_that_is_stale_today() {
        // "old" holds money but is not listed (stale): taking money out is fine.
        let s = state("0", &[("old", "100")]);

        assert_eq!(
            check(&move_action("old", "lend-a", 100), &s, Risk::High, RESTED),
            Ok(())
        );
    }

    #[test]
    fn low_risk_cannot_use_stable_lp_but_medium_can() {
        let s = state("100", &[]);
        let into_lp = rebalance(&[("lend-a", 50), ("lp", 50)]);

        assert_eq!(
            check(&into_lp, &s, Risk::Low, None),
            Err(RejectedRule::KindNotAllowed)
        );
        assert_eq!(check(&into_lp, &s, Risk::Medium, None), Ok(()));
    }

    #[test]
    fn rebalance_weights_must_sum_to_100() {
        let s = state("100", &[]);

        assert_eq!(
            check(
                &rebalance(&[("lend-a", 50), ("lend-b", 49)]),
                &s,
                Risk::High,
                None
            ),
            Err(RejectedRule::WeightsNot100)
        );
        // Summed in u32, so 200 + 200 does not wrap around.
        assert_eq!(
            check(
                &rebalance(&[("lend-a", 200), ("lend-b", 200)]),
                &s,
                Risk::High,
                None
            ),
            Err(RejectedRule::WeightsNot100)
        );
    }

    #[test]
    fn cooldown_counts_ticks_since_the_last_move() {
        let s = state("0", &[("lend-a", "100"), ("lend-b", "100")]);
        let small_move = move_action("lend-a", "lend-b", 10);

        // Low: 6 ticks.
        assert_eq!(
            check(&small_move, &s, Risk::Low, Some(5)),
            Err(RejectedRule::Cooldown)
        );
        assert_eq!(check(&small_move, &s, Risk::Low, Some(6)), Ok(()));
        // Medium: 3 ticks.
        assert_eq!(
            check(&small_move, &s, Risk::Medium, Some(2)),
            Err(RejectedRule::Cooldown)
        );
        assert_eq!(check(&small_move, &s, Risk::Medium, Some(3)), Ok(()));
        // High: may move every tick, never twice in the same tick.
        assert_eq!(
            check(&small_move, &s, Risk::High, Some(0)),
            Err(RejectedRule::Cooldown)
        );
        assert_eq!(check(&small_move, &s, Risk::High, Some(1)), Ok(()));
        // Never moved: no cooldown.
        assert_eq!(check(&small_move, &s, Risk::Low, None), Ok(()));
    }

    #[test]
    fn max_move_is_a_share_of_equity_not_of_the_source_village() {
        // Equity 1000. The target starts empty, so max share is not the limit.
        let s = state("0", &[("lend-a", "600"), ("old", "400")]);

        // Medium: 50% of equity = 500. 84% of 600 = 504, 83% = 498.
        assert_eq!(
            check(
                &move_action("lend-a", "lend-b", 84),
                &s,
                Risk::Medium,
                RESTED
            ),
            Err(RejectedRule::MaxMove)
        );
        assert_eq!(
            check(
                &move_action("lend-a", "lend-b", 83),
                &s,
                Risk::Medium,
                RESTED
            ),
            Ok(())
        );
        // High: anything.
        assert_eq!(
            check(
                &move_action("lend-a", "lend-b", 100),
                &s,
                Risk::High,
                RESTED
            ),
            Ok(())
        );
    }

    #[test]
    fn low_risk_moves_at_most_25_percent_of_equity() {
        // Equity 1000 spread so max share is not the limit.
        let s = state("0", &[("lend-a", "400"), ("lend-b", "300"), ("old", "300")]);

        // 84% of 300 = 252 > 250; 83% = 249.
        assert_eq!(
            check(&move_action("old", "lend-b", 84), &s, Risk::Low, RESTED),
            Err(RejectedRule::MaxMove)
        );
        assert_eq!(
            check(&move_action("old", "lend-b", 83), &s, Risk::Low, RESTED),
            Ok(())
        );
    }

    #[test]
    fn rebalance_counts_only_money_taken_out_of_villages() {
        // D29: a new avatar may invest all its cash in one tick.
        let all_cash = state("1000", &[]);
        assert_eq!(
            check(
                &rebalance(&[("lend-a", 50), ("lend-b", 50)]),
                &all_cash,
                Risk::Low,
                None
            ),
            Ok(())
        );

        // 600 in lend-a, 400 in lend-b. Target 30/70 takes 300 out of lend-a = 30%.
        let invested = state("0", &[("lend-a", "600"), ("lend-b", "400")]);
        let to_30_70 = rebalance(&[("lend-a", 30), ("lend-b", 70)]);
        assert_eq!(
            check(&to_30_70, &invested, Risk::Low, RESTED),
            Err(RejectedRule::MaxMove)
        );
        assert_eq!(check(&to_30_70, &invested, Risk::Medium, RESTED), Ok(()));
        // 45/55 takes 150 out = 15%: fine for low.
        let to_45_55 = rebalance(&[("lend-a", 45), ("lend-b", 55)]);
        assert_eq!(check(&to_45_55, &invested, Risk::Low, RESTED), Ok(()));
        // 0/100 takes 600 out = 60% > 50% (medium).
        let to_0_100 = rebalance(&[("lend-b", 100)]);
        assert_eq!(
            check(&to_0_100, &invested, Risk::Medium, RESTED),
            Err(RejectedRule::MaxMove)
        );
    }

    #[test]
    fn max_share_limits_one_village() {
        let s = state("1000", &[]);

        assert_eq!(
            check(&rebalance(&[("lend-a", 100)]), &s, Risk::Low, None),
            Err(RejectedRule::MaxShare)
        );
        assert_eq!(
            check(
                &rebalance(&[("lend-a", 61), ("lend-b", 39)]),
                &s,
                Risk::Low,
                None
            ),
            Err(RejectedRule::MaxShare)
        );
        assert_eq!(
            check(
                &rebalance(&[("lend-a", 60), ("lend-b", 40)]),
                &s,
                Risk::Low,
                None
            ),
            Ok(())
        );
        assert_eq!(
            check(
                &rebalance(&[("lend-a", 81), ("lend-b", 19)]),
                &s,
                Risk::Medium,
                None
            ),
            Err(RejectedRule::MaxShare)
        );
        assert_eq!(
            check(&rebalance(&[("lend-a", 100)]), &s, Risk::High, None),
            Ok(())
        );
    }

    #[test]
    fn move_checks_the_target_share_after_the_move() {
        // Equity 1000: lend-a 500, lend-b 500. Moving 20% of lend-a (100)
        // puts lend-b at 600 = 60% (ok for low); 21% puts it at 605 = 60.5%.
        let s = state("0", &[("lend-a", "500"), ("lend-b", "500")]);

        assert_eq!(
            check(&move_action("lend-a", "lend-b", 20), &s, Risk::Low, RESTED),
            Ok(())
        );
        assert_eq!(
            check(&move_action("lend-a", "lend-b", 21), &s, Risk::Low, RESTED),
            Err(RejectedRule::MaxShare)
        );
    }

    #[test]
    fn zero_equity_does_not_panic() {
        let s = state("0", &[]);

        assert_eq!(
            check(
                &rebalance(&[("lend-a", 50), ("lend-b", 50)]),
                &s,
                Risk::Low,
                None
            ),
            Ok(())
        );
    }

    #[test]
    fn rejected_rule_names_are_stable() {
        // These strings are stored in the database; changing one breaks history.
        assert_eq!(
            RejectedRule::VillageUnavailable.as_str(),
            "village_unavailable"
        );
        assert_eq!(RejectedRule::MaxShare.to_string(), "max_share");
    }
}
