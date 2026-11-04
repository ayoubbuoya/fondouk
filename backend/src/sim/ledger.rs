//! Paper ledger: how much cash an avatar holds and how much sits in each village.
//!
//! Pure (no database, no network), so the backtest today and the live sim on
//! Day 4 share exactly the same money rules. Costs (fees, slippage, gas) are
//! added with the cost model on Day 4; for now moving money is free.

use std::collections::BTreeMap;
use std::fmt;

use rust_decimal::{Decimal, MathematicalOps};

/// Days per year used to turn an APY into a daily growth factor.
const DAYS_PER_YEAR: i64 = 365;

/// Why a ledger operation was refused. The ledger is never changed when an
/// operation returns an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerError {
    /// REBALANCE weights must add up to exactly 100 (SPEC §5).
    WeightsDoNotSumTo100 { sum: u32 },
    /// A result did not fit in a `Decimal` (about 7.9 × 10^28). Not reachable
    /// with real amounts; reported instead of panicking.
    Overflow,
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WeightsDoNotSumTo100 { sum } => write!(f, "weights sum to {sum}, expected 100"),
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

    /// Moves all money so village `id` holds `weight`% of equity (REBALANCE).
    ///
    /// Weights must sum to exactly 100, so afterwards cash is 0. To avoid
    /// rounding creating or losing money, the last village gets
    /// "equity minus what was already given out", so the total stays exact.
    /// Villages with weight 0 are dropped.
    ///
    /// Which villages are *allowed* (stale, risk level, max share) is checked
    /// before this, by the backtest and later the guardrails (Day 3).
    ///
    /// # Errors
    /// `WeightsDoNotSumTo100`; the ledger is unchanged.
    pub fn rebalance(&mut self, weights: &BTreeMap<String, u8>) -> Result<(), LedgerError> {
        // Sum in u32: one hundred u8 weights of 255 would overflow a u8.
        let sum: u32 = weights.values().map(|&w| u32::from(w)).sum();
        if sum != 100 {
            return Err(LedgerError::WeightsDoNotSumTo100 { sum });
        }

        let equity = self.equity();
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
            new_positions.insert((*village_id).clone(), amount);
        }

        self.positions = new_positions;
        self.cash_usd = Decimal::ZERO;
        Ok(())
    }

    /// Grows every position by one day of yield (HANDOFF D21):
    /// `value * (1 + apy/100)^(1/365)`. Cash earns nothing.
    ///
    /// `apys` holds today's APY (percent) per village. A village with money but
    /// no APY today keeps growing at its last known APY (D23). A village never
    /// seen with an APY does not grow; that cannot happen in the backtest,
    /// because money only goes to villages that have data that day.
    ///
    /// # Errors
    /// `Overflow`; the ledger is unchanged.
    pub fn accrue_one_day(&mut self, apys: &BTreeMap<String, Decimal>) -> Result<(), LedgerError> {
        let mut last_known_apy = self.last_known_apy.clone();
        last_known_apy.extend(apys.iter().map(|(id, apy)| (id.clone(), *apy)));

        let mut grown = BTreeMap::new();
        for (village_id, value) in &self.positions {
            let new_value = match last_known_apy.get(village_id) {
                Some(apy) => value
                    .checked_mul(daily_growth_factor(*apy)?)
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

/// `(1 + apy/100)^(1/365)`: what 1 dollar becomes after one day.
///
/// The APY already includes compounding, so the daily factor is its 365th
/// root, not `apy/100/365` (that would overstate 14% APY as about 15%).
fn daily_growth_factor(apy_percent: Decimal) -> Result<Decimal, LedgerError> {
    let yearly = Decimal::ONE + apy_percent / Decimal::ONE_HUNDRED;
    let one_day = Decimal::ONE / Decimal::from(DAYS_PER_YEAR);
    yearly.checked_powd(one_day).ok_or(LedgerError::Overflow)
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
        pairs.iter().map(|(id, apy)| (id.to_string(), d(apy))).collect()
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
            .rebalance(&weights(&[("lend-usdc", 60), ("lend-usdt", 40)]))
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
            .rebalance(&weights(&[("a", 33), ("b", 33), ("c", 34)]))
            .unwrap();
        ledger.rebalance(&weights(&[("a", 1), ("b", 99)])).unwrap();

        assert_eq!(ledger.equity(), d("100"));
    }

    #[test]
    fn rebalance_drops_zero_weight_villages() {
        let mut ledger = Ledger::new(d("100"));

        ledger.rebalance(&weights(&[("a", 0), ("b", 100)])).unwrap();

        assert_eq!(ledger.positions().len(), 1);
        assert_eq!(ledger.positions()["b"], d("100"));
    }

    #[test]
    fn weights_not_summing_to_100_are_refused_and_ledger_is_unchanged() {
        let mut ledger = Ledger::new(d("100"));
        let before = ledger.clone();

        let err = ledger
            .rebalance(&weights(&[("a", 60), ("b", 39)]))
            .unwrap_err();

        assert_eq!(err, LedgerError::WeightsDoNotSumTo100 { sum: 99 });
        assert_eq!(ledger, before);
    }

    #[test]
    fn large_weights_are_summed_without_u8_overflow() {
        let mut ledger = Ledger::new(d("100"));

        let err = ledger
            .rebalance(&weights(&[("a", 200), ("b", 200)]))
            .unwrap_err();

        assert_eq!(err, LedgerError::WeightsDoNotSumTo100 { sum: 400 });
    }

    #[test]
    fn one_year_at_14_percent_apy_gives_14_percent() {
        let mut ledger = Ledger::new(d("10000"));
        ledger.rebalance(&weights(&[("lend-usdc", 100)])).unwrap();

        for _ in 0..365 {
            ledger.accrue_one_day(&apys(&[("lend-usdc", "14")])).unwrap();
        }

        assert!(within_a_cent(ledger.equity(), d("11400")), "{}", ledger.equity());
    }

    #[test]
    fn cash_does_not_earn_yield() {
        let mut ledger = Ledger::new(d("10000"));

        ledger.accrue_one_day(&apys(&[("lend-usdc", "14")])).unwrap();

        assert_eq!(ledger.equity(), d("10000"));
    }

    #[test]
    fn a_village_without_data_today_grows_at_its_last_known_apy() {
        let mut with_gap = Ledger::new(d("10000"));
        with_gap.rebalance(&weights(&[("lend-usdc", 100)])).unwrap();
        let mut without_gap = with_gap.clone();

        with_gap.accrue_one_day(&apys(&[("lend-usdc", "10")])).unwrap();
        with_gap.accrue_one_day(&apys(&[])).unwrap(); // no data today
        without_gap.accrue_one_day(&apys(&[("lend-usdc", "10")])).unwrap();
        without_gap.accrue_one_day(&apys(&[("lend-usdc", "10")])).unwrap();

        assert_eq!(with_gap.equity(), without_gap.equity());
    }

    #[test]
    fn a_village_never_seen_with_data_does_not_grow() {
        let mut ledger = Ledger::new(d("100"));
        ledger.rebalance(&weights(&[("unknown", 100)])).unwrap();

        ledger.accrue_one_day(&apys(&[])).unwrap();

        assert_eq!(ledger.equity(), d("100"));
    }

    #[test]
    fn rebalance_keeps_equity_after_growth() {
        let mut ledger = Ledger::new(d("10000"));
        ledger.rebalance(&weights(&[("a", 100)])).unwrap();
        ledger.accrue_one_day(&apys(&[("a", "12")])).unwrap();
        let grown = ledger.equity();

        ledger.rebalance(&weights(&[("a", 50), ("b", 50)])).unwrap();

        assert_eq!(ledger.equity(), grown);
    }
}
