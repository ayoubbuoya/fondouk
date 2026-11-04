//! Agent: what an avatar sees (`MarketState`), what it may answer (`Action`)
//! and, for now, a mock decision rule.
//!
//! "The LLM proposes, code decides": on Day 3 the LLM returns an `Action` as
//! JSON and Rust guardrails check it. Until then `mock_decide` stands in for
//! the LLM so the backtest can be built and tested without network or cost.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// The fixed action set (SPEC §5). JSON looks like
/// `{"action": "MOVE", "from": "...", "to": "...", "percent": 25, "reason": "..."}`.
///
/// Field limits (`percent` 1..=100, `reason` <= 140 chars, weights summing to
/// 100 over allowed villages) are checked by the guardrails (Day 3), not by
/// parsing: a parsed action is a *proposal*, not yet an allowed one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Action {
    Hold {
        reason: String,
    },
    Move {
        from: String,
        to: String,
        percent: u8,
        reason: String,
    },
    Rebalance {
        /// Village id -> percent of equity.
        weights: BTreeMap<String, u8>,
        reason: String,
    },
    /// Arena message; never changes positions.
    Say {
        text: String,
        reason: String,
    },
}

/// One village as the avatar sees it on a given day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VillageQuote {
    pub village_id: String,
    /// `lending` or `stable_lp` (used by the risk guardrails on Day 3).
    pub kind: String,
    /// Total APY in percent: base + reward.
    pub apy: Decimal,
    pub tvl_usd: Decimal,
}

/// Everything a decision may look at for one day (backtest) or tick (live).
///
/// Only villages that have fresh data are listed: a village missing here
/// cannot receive money that day (D23, and the stale rule live).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketState {
    pub day: NaiveDate,
    pub villages: Vec<VillageQuote>,
    pub cash_usd: Decimal,
    pub positions: BTreeMap<String, Decimal>,
}

/// Temporary stand-in for the LLM: put everything in the village with the
/// highest APY today. Returns `HOLD` when already fully there (no pointless
/// moves) or when no village has data.
///
/// Ties go to the first village in `villages` order, so the result is
/// deterministic.
pub fn mock_decide(state: &MarketState) -> Action {
    let mut best: Option<&VillageQuote> = None;
    for quote in &state.villages {
        if best.is_none_or(|current| quote.apy > current.apy) {
            best = Some(quote);
        }
    }
    let Some(best) = best else {
        return Action::Hold {
            reason: "No village has data today.".to_string(),
        };
    };

    let already_all_in = state.cash_usd.is_zero()
        && state.positions.len() == 1
        && state.positions.contains_key(&best.village_id);
    if already_all_in {
        return Action::Hold {
            reason: format!("Staying in {}, still the best yield.", best.village_id),
        };
    }

    Action::Rebalance {
        weights: BTreeMap::from([(best.village_id.clone(), 100)]),
        reason: format!("Moving all to {}, highest yield today.", best.village_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn state(villages: Vec<VillageQuote>, cash: &str, positions: &[(&str, &str)]) -> MarketState {
        MarketState {
            day: NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(),
            villages,
            cash_usd: d(cash),
            positions: positions
                .iter()
                .map(|(id, value)| (id.to_string(), d(value)))
                .collect(),
        }
    }

    /// Parses `json`, then checks that serializing gives the same JSON back.
    fn round_trip(json: &str) -> Action {
        let action: Action = serde_json::from_str(json).unwrap();
        let back: serde_json::Value = serde_json::to_value(&action).unwrap();
        let original: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(back, original);
        action
    }

    #[test]
    fn spec_hold_example_round_trips() {
        let action = round_trip(r#"{ "action": "HOLD", "reason": "..." }"#);
        assert!(matches!(action, Action::Hold { .. }));
    }

    #[test]
    fn spec_move_example_round_trips() {
        let action = round_trip(
            r#"{ "action": "MOVE", "from": "village_id", "to": "village_id", "percent": 25, "reason": "..." }"#,
        );
        assert!(matches!(action, Action::Move { percent: 25, .. }));
    }

    #[test]
    fn spec_rebalance_example_round_trips() {
        let action = round_trip(
            r#"{ "action": "REBALANCE", "weights": { "village_a": 60, "village_b": 40 }, "reason": "..." }"#,
        );
        let Action::Rebalance { weights, .. } = action else {
            panic!("expected REBALANCE");
        };
        assert_eq!(weights["village_a"], 60);
    }

    #[test]
    fn spec_say_example_round_trips() {
        let action = round_trip(r#"{ "action": "SAY", "text": "...", "reason": "..." }"#);
        assert!(matches!(action, Action::Say { .. }));
    }

    #[test]
    fn unknown_action_or_negative_percent_fails_to_parse() {
        assert!(serde_json::from_str::<Action>(r#"{"action":"YOLO","reason":"x"}"#).is_err());
        assert!(
            serde_json::from_str::<Action>(
                r#"{"action":"MOVE","from":"a","to":"b","percent":-5,"reason":"x"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn mock_moves_everything_to_the_highest_apy() {
        let s = state(vec![quote("a", "8"), quote("b", "14"), quote("c", "11")], "10000", &[]);

        let Action::Rebalance { weights, .. } = mock_decide(&s) else {
            panic!("expected REBALANCE");
        };
        assert_eq!(weights, BTreeMap::from([("b".to_string(), 100)]));
    }

    #[test]
    fn mock_holds_when_already_all_in_the_best_village() {
        let s = state(vec![quote("a", "8"), quote("b", "14")], "0", &[("b", "10000")]);

        assert!(matches!(mock_decide(&s), Action::Hold { .. }));
    }

    #[test]
    fn mock_moves_when_part_of_the_money_is_elsewhere() {
        let s = state(vec![quote("a", "8"), quote("b", "14")], "0", &[("a", "1"), ("b", "9")]);

        assert!(matches!(mock_decide(&s), Action::Rebalance { .. }));
    }

    #[test]
    fn mock_holds_when_no_village_has_data() {
        let s = state(vec![], "10000", &[]);

        assert!(matches!(mock_decide(&s), Action::Hold { .. }));
    }

    #[test]
    fn mock_breaks_ties_by_list_order() {
        let s = state(vec![quote("a", "10"), quote("b", "10")], "100", &[]);

        let Action::Rebalance { weights, .. } = mock_decide(&s) else {
            panic!("expected REBALANCE");
        };
        assert!(weights.contains_key("a"));
    }
}
