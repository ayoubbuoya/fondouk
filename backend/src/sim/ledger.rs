//! Paper ledger: how much cash an avatar holds and how much sits in each village.
//!
//! Pure (no database, no network), so the backtest and the live sim share
//! exactly the same money rules. Moves pay the cost estimated by
//! `sim::costs`; it is taken from the money that arrives.

use std::collections::BTreeMap;
use std::fmt;

use rust_decimal::{Decimal, MathematicalOps};

use crate::agent::Action;

/// Seconds in one day.
const SECONDS_PER_DAY: u64 = 86_400;

/// Seconds per 365-day year, used to turn an APY into a growth factor for
/// any duration (SPEC F6.2). One day is exactly 1/365 of it.
const SECONDS_PER_YEAR: u64 = 31_536_000;

/// Why a ledger operation was refused. The ledger is never changed when an
/// operation returns an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerError {
    /// REBALANCE weights must add up to exactly 100 (SPEC §5).
    WeightsDoNotSumTo100 { sum: u32 },
    /// MOVE `percent` must be 1 to 100 (SPEC §5).
    InvalidPercent { percent: u8 },
    /// MOVE from a village where the avatar holds nothing.
    NoPosition { village_id: String },
    /// The cost is larger than the money moved (MOVE) or than equity
    /// (REBALANCE). The guardrails refuse such moves before this.
    CostExceedsAmount,
    /// A result did not fit in a `Decimal` (about 7.9 × 10^28). Not reachable
    /// with real amounts; reported instead of panicking.
    Overflow,
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WeightsDoNotSumTo100 { sum } => write!(f, "weights sum to {sum}, expected 100"),
            Self::InvalidPercent { percent } => {
                write!(f, "percent {percent} is not between 1 and 100")
            }
            Self::NoPosition { village_id } => write!(f, "no money in village {village_id}"),
            Self::CostExceedsAmount => write!(f, "cost is larger than the money moved"),
            Self::Overflow => write!(f, "amount too large for the ledger"),
        }
    }
}

impl std::error::Error for LedgerError {}

/// Cash plus one USD position per village.
///
/// `BTreeMap` keeps villages sorted by id, so iteration order (and therefore
/// tests, logs and future input hashes) is always the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ledger {
    cash_usd: Decimal,
    /// Only villages holding money are listed (no zero entries).
    positions: BTreeMap<String, Decimal>,
    /// Last APY seen for each village, used when a day has no data (D23).
    last_known_apy: BTreeMap<String, Decimal>,
}

impl Ledger {
    /// A new ledger with everything in cash (a new avatar, Q-E).
    pub fn new(cash_usd: Decimal) -> Self {
        Self {
            cash_usd,
            positions: BTreeMap::new(),
            last_known_apy: BTreeMap::new(),
        }
    }

    /// A ledger rebuilt from stored state (live sim: `avatars.cash_usd` and
    /// the `positions` rows). Empty positions are dropped. No APY is known
    /// yet; the caller passes current APYs to `accrue_seconds`.
    pub fn from_parts(cash_usd: Decimal, positions: BTreeMap<String, Decimal>) -> Self {
        Self {
            cash_usd,
            positions: positions
                .into_iter()
                .filter(|(_, value)| *value > Decimal::ZERO)
                .collect(),
            last_known_apy: BTreeMap::new(),
        }
    }

    pub fn cash_usd(&self) -> Decimal {
        self.cash_usd
    }

    pub fn positions(&self) -> &BTreeMap<String, Decimal> {
        &self.positions
    }

    /// Total value: cash plus every position.
    pub fn equity(&self) -> Decimal {
        self.cash_usd + self.positions.values().copied().sum::<Decimal>()
    }

    /// Applies an already-allowed action with its estimated `cost_usd`.
    /// Returns `true` when money moved (MOVE or REBALANCE), so the caller can
    /// start the cooldown. HOLD and SAY change nothing.
    ///
    /// # Errors
    /// The ledger errors of `move_percent` / `rebalance`; the ledger is
    /// unchanged.
    pub fn apply_action(
        &mut self,
        action: &Action,
        cost_usd: Decimal,
    ) -> Result<bool, LedgerError> {
        match action {
            Action::Hold { .. } | Action::Say { .. } => Ok(false),
            Action::Move {
                from, to, percent, ..
            } => self
                .move_percent(from, to, *percent, cost_usd)
                .map(|()| true),
            Action::Rebalance { weights, .. } => self.rebalance(weights, cost_usd).map(|()| true),
        }
    }

    /// Moves all money so village `id` holds `weight`% of equity (REBALANCE),
    /// after paying `cost_usd`.
    ///
    /// Weights must sum to exactly 100, so afterwards cash is 0. The cost is
    /// paid first, so the villages share `equity - cost`. To avoid rounding
    /// creating or losing money, the last village gets "what is left minus
    /// what was already given out", so the total stays exact. Villages with
    /// weight 0 (or an amount of 0) are dropped.
    ///
    /// Which villages are *allowed* (stale, risk level, max share, costs) is
    /// checked before this by the guardrails.
    ///
    /// # Errors
    /// `WeightsDoNotSumTo100` or `CostExceedsAmount`; the ledger is unchanged.
    pub fn rebalance(
        &mut self,
        weights: &BTreeMap<String, u8>,
        cost_usd: Decimal,
    ) -> Result<(), LedgerError> {
        // Sum in u32: one hundred u8 weights of 255 would overflow a u8.
        let sum: u32 = weights.values().map(|&w| u32::from(w)).sum();
        if sum != 100 {
            return Err(LedgerError::WeightsDoNotSumTo100 { sum });
        }
        if cost_usd > self.equity() {
            return Err(LedgerError::CostExceedsAmount);
        }

        let equity = self.equity() - cost_usd;
        let funded: Vec<(&String, u8)> = weights
            .iter()
            .filter(|(_, w)| **w > 0)
            .map(|(id, w)| (id, *w))
            .collect();

        let mut new_positions = BTreeMap::new();
        let mut given_out = Decimal::ZERO;
        for (index, (village_id, weight)) in funded.iter().enumerate() {
            let is_last = index == funded.len() - 1;
            let amount = if is_last {
                equity - given_out
            } else {
                equity * Decimal::from(*weight) / Decimal::ONE_HUNDRED
            };
            given_out += amount;
            if amount > Decimal::ZERO {
                new_positions.insert((*village_id).clone(), amount);
            }
        }

        self.positions = new_positions;
        self.cash_usd = Decimal::ZERO;
        Ok(())
    }

    /// Moves `percent`% of the money in village `from` to village `to` (MOVE),
    /// paying `cost_usd` out of the money that moves.
    ///
    /// Cash is not a village (HANDOFF D27): cash is invested with REBALANCE.
    /// At 100% the whole position moves, so no dust is left behind. Equity
    /// goes down by exactly `cost_usd`.
    ///
    /// Which targets are *allowed* is checked before this by the guardrails.
    ///
    /// # Errors
    /// `InvalidPercent`, `NoPosition` or `CostExceedsAmount`; the ledger is
    /// unchanged.
    pub fn move_percent(
        &mut self,
        from: &str,
        to: &str,
        percent: u8,
        cost_usd: Decimal,
    ) -> Result<(), LedgerError> {
        if !(1..=100).contains(&percent) {
            return Err(LedgerError::InvalidPercent { percent });
        }
        let source = self.positions.get(from).copied().unwrap_or_default();
        if source <= Decimal::ZERO {
            return Err(LedgerError::NoPosition {
                village_id: from.to_string(),
            });
        }
        if from == to {
            return Ok(()); // nothing changes; the guardrails refuse this anyway
        }

        let amount = if percent == 100 {
            source
        } else {
            source * Decimal::from(percent) / Decimal::ONE_HUNDRED
        };
        if cost_usd > amount {
            return Err(LedgerError::CostExceedsAmount);
        }
        let arriving = amount - cost_usd;

        let left = source - amount;
        if left.is_zero() {
            self.positions.remove(from);
        } else {
            self.positions.insert(from.to_string(), left);
        }
        // Keep the "no zero entries" rule (and the `positions.value_usd > 0` CHECK).
        if arriving > Decimal::ZERO {
            *self.positions.entry(to.to_string()).or_default() += arriving;
        }
        Ok(())
    }

    /// Grows every position by one day of yield (HANDOFF D21):
    /// `value * (1 + apy/100)^(1/365)`. Same as `accrue_seconds` for 86 400 s.
    ///
    /// # Errors
    /// `Overflow`; the ledger is unchanged.
    pub fn accrue_one_day(&mut self, apys: &BTreeMap<String, Decimal>) -> Result<(), LedgerError> {
        self.accrue_seconds(apys, SECONDS_PER_DAY)
    }

    /// Grows every position by `seconds` of yield (SPEC F6.2):
    /// `value * (1 + apy/100)^(seconds / 31_536_000)`. Cash earns nothing.
    ///
    /// `apys` holds the current APY (percent) per village. A village with
    /// money but no APY here keeps growing at its last known APY (D23). A
    /// village never seen with an APY does not grow; that cannot happen in
    /// the backtest, because money only goes to villages that have data that
    /// day, and live every village with money has a stored snapshot.
    ///
    /// # Errors
    /// `Overflow`; the ledger is unchanged.
    pub fn accrue_seconds(
        &mut self,
        apys: &BTreeMap<String, Decimal>,
        seconds: u64,
    ) -> Result<(), LedgerError> {
        let mut last_known_apy = self.last_known_apy.clone();
        last_known_apy.extend(apys.iter().map(|(id, apy)| (id.clone(), *apy)));

        let mut grown = BTreeMap::new();
        for (village_id, value) in &self.positions {
            let new_value = match last_known_apy.get(village_id) {
                Some(apy) => value
                    .checked_mul(growth_factor(*apy, seconds)?)
                    .ok_or(LedgerError::Overflow)?,
                None => *value,
            };
            grown.insert(village_id.clone(), new_value);
        }

        // Only now, after every step succeeded, change the ledger.
        self.positions = grown;
        self.last_known_apy = last_known_apy;
        Ok(())
    }
}

/// `(1 + apy/100)^(seconds / 31_536_000)`: what 1 dollar becomes after
/// `seconds`.
///
/// The APY already includes compounding, so the factor is a root of the
/// yearly factor, not `apy/100 × fraction of a year` (that would overstate
/// 14% APY as about 15%).
fn growth_factor(apy_percent: Decimal, seconds: u64) -> Result<Decimal, LedgerError> {
    let yearly = Decimal::ONE + apy_percent / Decimal::ONE_HUNDRED;
    let share_of_year = Decimal::from(seconds) / Decimal::from(SECONDS_PER_YEAR);
    yearly
        .checked_powd(share_of_year)
        .ok_or(LedgerError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    fn weights(pairs: &[(&str, u8)]) -> BTreeMap<String, u8> {
        pairs.iter().map(|(id, w)| (id.to_string(), *w)).collect()
    }

    fn apys(pairs: &[(&str, &str)]) -> BTreeMap<String, Decimal> {
        pairs
            .iter()
            .map(|(id, apy)| (id.to_string(), d(apy)))
            .collect()
    }

    /// True when `a` and `b` differ by at most one cent.
    fn within_a_cent(a: Decimal, b: Decimal) -> bool {
        (a - b).abs() <= d("0.01")
    }

    #[test]
    fn new_ledger_holds_everything_in_cash() {
        let ledger = Ledger::new(d("10000"));

        assert_eq!(ledger.cash_usd(), d("10000"));
        assert!(ledger.positions().is_empty());
        assert_eq!(ledger.equity(), d("10000"));
    }

    #[test]
    fn rebalance_splits_equity_by_weight() {
        let mut ledger = Ledger::new(d("10000"));

        ledger
            .rebalance(
                &weights(&[("lend-usdc", 60), ("lend-usdt", 40)]),
                Decimal::ZERO,
            )
            .unwrap();

        assert_eq!(ledger.positions()["lend-usdc"], d("6000"));
        assert_eq!(ledger.positions()["lend-usdt"], d("4000"));
        assert_eq!(ledger.cash_usd(), Decimal::ZERO);
    }

    #[test]
    fn rebalance_never_creates_or_loses_money() {
        // 1/3 splits cannot be exact in decimal; the last village takes the remainder.
        let mut ledger = Ledger::new(d("100"));

        ledger
            .rebalance(&weights(&[("a", 33), ("b", 33), ("c", 34)]), Decimal::ZERO)
            .unwrap();
        ledger
            .rebalance(&weights(&[("a", 1), ("b", 99)]), Decimal::ZERO)
            .unwrap();

        assert_eq!(ledger.equity(), d("100"));
    }

    #[test]
    fn rebalance_drops_zero_weight_villages() {
        let mut ledger = Ledger::new(d("100"));

        ledger
            .rebalance(&weights(&[("a", 0), ("b", 100)]), Decimal::ZERO)
            .unwrap();

        assert_eq!(ledger.positions().len(), 1);
        assert_eq!(ledger.positions()["b"], d("100"));
    }

    #[test]
    fn weights_not_summing_to_100_are_refused_and_ledger_is_unchanged() {
        let mut ledger = Ledger::new(d("100"));
        let before = ledger.clone();

        let err = ledger
            .rebalance(&weights(&[("a", 60), ("b", 39)]), Decimal::ZERO)
            .unwrap_err();

        assert_eq!(err, LedgerError::WeightsDoNotSumTo100 { sum: 99 });
        assert_eq!(ledger, before);
    }

    #[test]
    fn large_weights_are_summed_without_u8_overflow() {
        let mut ledger = Ledger::new(d("100"));

        let err = ledger
            .rebalance(&weights(&[("a", 200), ("b", 200)]), Decimal::ZERO)
            .unwrap_err();

        assert_eq!(err, LedgerError::WeightsDoNotSumTo100 { sum: 400 });
    }

    #[test]
    fn one_year_at_14_percent_apy_gives_14_percent() {
        let mut ledger = Ledger::new(d("10000"));
        ledger
            .rebalance(&weights(&[("lend-usdc", 100)]), Decimal::ZERO)
            .unwrap();

        for _ in 0..365 {
            ledger
                .accrue_one_day(&apys(&[("lend-usdc", "14")]))
                .unwrap();
        }

        assert!(
            within_a_cent(ledger.equity(), d("11400")),
            "{}",
            ledger.equity()
        );
    }

    #[test]
    fn cash_does_not_earn_yield() {
        let mut ledger = Ledger::new(d("10000"));

        ledger
            .accrue_one_day(&apys(&[("lend-usdc", "14")]))
            .unwrap();

        assert_eq!(ledger.equity(), d("10000"));
    }

    #[test]
    fn a_village_without_data_today_grows_at_its_last_known_apy() {
        let mut with_gap = Ledger::new(d("10000"));
        with_gap
            .rebalance(&weights(&[("lend-usdc", 100)]), Decimal::ZERO)
            .unwrap();
        let mut without_gap = with_gap.clone();

        with_gap
            .accrue_one_day(&apys(&[("lend-usdc", "10")]))
            .unwrap();
        with_gap.accrue_one_day(&apys(&[])).unwrap(); // no data today
        without_gap
            .accrue_one_day(&apys(&[("lend-usdc", "10")]))
            .unwrap();
        without_gap
            .accrue_one_day(&apys(&[("lend-usdc", "10")]))
            .unwrap();

        assert_eq!(with_gap.equity(), without_gap.equity());
    }

    #[test]
    fn a_village_never_seen_with_data_does_not_grow() {
        let mut ledger = Ledger::new(d("100"));
        ledger
            .rebalance(&weights(&[("unknown", 100)]), Decimal::ZERO)
            .unwrap();

        ledger.accrue_one_day(&apys(&[])).unwrap();

        assert_eq!(ledger.equity(), d("100"));
    }

    #[test]
    fn move_percent_moves_part_of_one_village() {
        let mut ledger = Ledger::new(d("1000"));
        ledger
            .rebalance(&weights(&[("a", 100)]), Decimal::ZERO)
            .unwrap();

        ledger.move_percent("a", "b", 25, Decimal::ZERO).unwrap();

        assert_eq!(ledger.positions()["a"], d("750"));
        assert_eq!(ledger.positions()["b"], d("250"));
        assert_eq!(ledger.equity(), d("1000"));
    }

    #[test]
    fn move_100_percent_empties_the_source() {
        let mut ledger = Ledger::new(d("100"));
        ledger
            .rebalance(&weights(&[("a", 33), ("b", 67)]), Decimal::ZERO)
            .unwrap();

        ledger.move_percent("a", "b", 100, Decimal::ZERO).unwrap();

        assert!(!ledger.positions().contains_key("a"));
        assert_eq!(ledger.positions()["b"], d("100"));
    }

    #[test]
    fn move_from_cash_or_an_empty_village_is_refused_and_ledger_is_unchanged() {
        let mut ledger = Ledger::new(d("100"));
        let before = ledger.clone();

        assert_eq!(
            ledger.move_percent("cash", "a", 50, Decimal::ZERO),
            Err(LedgerError::NoPosition {
                village_id: "cash".to_string()
            })
        );
        assert_eq!(
            ledger.move_percent("a", "b", 0, Decimal::ZERO),
            Err(LedgerError::InvalidPercent { percent: 0 })
        );
        assert_eq!(ledger, before);
    }

    #[test]
    fn rebalance_keeps_equity_after_growth() {
        let mut ledger = Ledger::new(d("10000"));
        ledger
            .rebalance(&weights(&[("a", 100)]), Decimal::ZERO)
            .unwrap();
        ledger.accrue_one_day(&apys(&[("a", "12")])).unwrap();
        let grown = ledger.equity();

        ledger
            .rebalance(&weights(&[("a", 50), ("b", 50)]), Decimal::ZERO)
            .unwrap();

        assert_eq!(ledger.equity(), grown);
    }

    #[test]
    fn accruing_one_day_in_seconds_equals_accrue_one_day() {
        let mut by_day = Ledger::new(d("10000"));
        by_day
            .rebalance(&weights(&[("a", 100)]), Decimal::ZERO)
            .unwrap();
        let mut by_seconds = by_day.clone();

        by_day.accrue_one_day(&apys(&[("a", "14")])).unwrap();
        by_seconds
            .accrue_seconds(&apys(&[("a", "14")]), 86_400)
            .unwrap();

        assert_eq!(by_day.equity(), by_seconds.equity());
    }

    #[test]
    fn ten_minutes_of_yield_follow_the_f6_2_formula() {
        let mut ledger = Ledger::new(d("10000"));
        ledger
            .rebalance(&weights(&[("a", 100)]), Decimal::ZERO)
            .unwrap();

        ledger.accrue_seconds(&apys(&[("a", "14")]), 600).unwrap();

        // 10000 × 1.14^(600 / 31 536 000) ≈ 10000.0249
        let expected = d("10000") * d("1.14").powd(d("600") / d("31536000"));
        assert_eq!(ledger.equity(), expected);
        assert_eq!(ledger.equity().round_dp(4), d("10000.0249"));
    }

    #[test]
    fn rebalance_pays_the_cost_first_and_splits_the_rest() {
        let mut ledger = Ledger::new(d("1000"));

        ledger
            .rebalance(&weights(&[("a", 50), ("b", 50)]), d("10"))
            .unwrap();

        assert_eq!(ledger.positions()["a"], d("495"));
        assert_eq!(ledger.positions()["b"], d("495"));
        assert_eq!(ledger.equity(), d("990"));
    }

    #[test]
    fn move_takes_the_cost_from_the_money_that_arrives() {
        let mut ledger = Ledger::new(d("1000"));
        ledger
            .rebalance(&weights(&[("a", 100)]), Decimal::ZERO)
            .unwrap();

        ledger.move_percent("a", "b", 50, d("1.25")).unwrap();

        assert_eq!(ledger.positions()["a"], d("500"));
        assert_eq!(ledger.positions()["b"], d("498.75"));
        assert_eq!(ledger.equity(), d("998.75"));
    }

    #[test]
    fn a_cost_larger_than_the_money_moved_is_refused_and_ledger_is_unchanged() {
        let mut ledger = Ledger::new(d("100"));
        ledger
            .rebalance(&weights(&[("a", 100)]), Decimal::ZERO)
            .unwrap();
        let before = ledger.clone();

        assert_eq!(
            ledger.move_percent("a", "b", 10, d("10.01")),
            Err(LedgerError::CostExceedsAmount)
        );
        assert_eq!(
            ledger.rebalance(&weights(&[("b", 100)]), d("100.01")),
            Err(LedgerError::CostExceedsAmount)
        );
        assert_eq!(ledger, before);
    }

    #[test]
    fn a_move_whose_cost_eats_everything_leaves_no_empty_position() {
        let mut ledger = Ledger::new(d("100"));
        ledger
            .rebalance(&weights(&[("a", 100)]), Decimal::ZERO)
            .unwrap();

        ledger.move_percent("a", "b", 10, d("10")).unwrap();

        assert!(!ledger.positions().contains_key("b"));
        assert_eq!(ledger.equity(), d("90"));
    }

    #[test]
    fn from_parts_drops_empty_positions() {
        let ledger = Ledger::from_parts(
            d("5"),
            BTreeMap::from([("a".to_string(), d("10")), ("b".to_string(), Decimal::ZERO)]),
        );

        assert_eq!(ledger.positions().len(), 1);
        assert_eq!(ledger.equity(), d("15"));
    }

    #[test]
    fn apply_action_reports_whether_money_moved() {
        let mut ledger = Ledger::new(d("100"));
        let hold = Action::Hold {
            reason: "x".to_string(),
        };
        let invest = Action::Rebalance {
            weights: weights(&[("a", 100)]),
            reason: "x".to_string(),
        };

        assert_eq!(ledger.apply_action(&hold, Decimal::ZERO), Ok(false));
        assert_eq!(ledger.apply_action(&invest, d("0.02")), Ok(true));
        assert_eq!(ledger.equity(), d("99.98"));
    }
}
