//! Cost model (SPEC F6.3, HANDOFF D31): what moving money between villages
//! costs, in USD.
//!
//! Pure (no database, no network), shared by the live tick and the backtest.
//!
//! A move is split into steps, and each step costs something only when it
//! really happens on Rhea:
//!
//! ```text
//! source ──① exit──► token in hand ──② swap? ──► token the target accepts ──③ enter──► target
//!
//! ① exit   cash: free (cash is USDC)    lending: gas    LP: fee + slippage + gas
//! ② swap   only when the target does not accept the token in hand,
//!          through the stable pool `SWAP_VILLAGE_ID`: fee + slippage + gas
//! ③ enter  lending: gas                 LP: fee + slippage + gas
//!
//! slippage = amount × SLIPPAGE_K × amount / TVL
//! ```
//!
//! Exit and enter are costed on each village's **total** (one transaction per
//! village). Swaps are costed per piece of money: sources are matched with
//! targets in a fixed order (cash first, then by village id), so the same
//! input always gives the same cost.
//!
//! The numbers are estimates checked on 6 Nov 2026 (HANDOFF D31). They err
//! on the expensive side: Rhea's stable pools have less price impact than
//! the linear slippage used here.

use std::collections::BTreeSet;

use rust_decimal::Decimal;

use crate::agent::{Action, MarketState, VillageQuote};

/// Gas per transaction in USD: about 0.005 NEAR at $3.66 (6 Nov 2026).
pub const GAS_USD_PER_TX: Decimal = Decimal::from_parts(2, 0, 0, false, 2); // 0.02

/// Slippage factor `k` in `amount × k × amount / TVL`.
pub const SLIPPAGE_K: Decimal = Decimal::from_parts(1, 0, 0, false, 1); // 0.1

/// The token cash is held in (sim and real mode deposit USDC).
pub const CASH_TOKEN: &str = "USDC";

/// The stable pool used to swap one stablecoin into another (Rhea pool 4179,
/// USDT / USDC / USDT.e / USDC.e).
pub const SWAP_VILLAGE_ID: &str = "rhea-dex-stable4";

const BPS_PER_UNIT: u32 = 10_000;

/// The swap a move needs cannot be done: the swap pool has no fresh quote
/// (stale) or does not trade the two tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoSwapRoute;

/// Where moved money comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Cash,
    Village(String),
}

/// The money an action moves: how much leaves each source and how much
/// arrives in each target, before costs.
///
/// Sources are listed cash first, then by village id; targets by village id.
/// The two totals are equal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flows {
    pub sources: Vec<(Source, Decimal)>,
    pub targets: Vec<(String, Decimal)>,
}

impl Flows {
    /// Total money moved (the same as the sum of the sources).
    pub fn moved(&self) -> Decimal {
        self.targets.iter().map(|(_, amount)| *amount).sum()
    }
}

/// The money `action` would move from `state`. HOLD and SAY move nothing.
///
/// Field checks (percent range, weights summing to 100, money in the source)
/// are the guardrails' job and happen before this; invalid input just gives
/// fewer or empty flows here.
pub fn flows_of(action: &Action, state: &MarketState) -> Flows {
    match action {
        Action::Hold { .. } | Action::Say { .. } => Flows::default(),
        Action::Move {
            from, to, percent, ..
        } => {
            let source = state.positions.get(from).copied().unwrap_or_default();
            let amount = if *percent >= 100 {
                source
            } else {
                source * Decimal::from(*percent) / Decimal::ONE_HUNDRED
            };
            if amount <= Decimal::ZERO || from == to {
                return Flows::default();
            }
            Flows {
                sources: vec![(Source::Village(from.clone()), amount)],
                targets: vec![(to.clone(), amount)],
            }
        }
        Action::Rebalance { weights, .. } => {
            let equity = state.cash_usd + state.positions.values().copied().sum::<Decimal>();
            let mut flows = Flows::default();
            // Weights sum to 100, so all cash is put to work.
            if state.cash_usd > Decimal::ZERO {
                flows.sources.push((Source::Cash, state.cash_usd));
            }
            // Every village that holds money now or should after: sorted by id.
            let village_ids: BTreeSet<&String> =
                state.positions.keys().chain(weights.keys()).collect();
            for village_id in village_ids {
                let current = state.positions.get(village_id).copied().unwrap_or_default();
                let weight = weights.get(village_id).copied().unwrap_or(0);
                let target = equity * Decimal::from(weight) / Decimal::ONE_HUNDRED;
                if current > target {
                    flows
                        .sources
                        .push((Source::Village(village_id.clone()), current - target));
                } else if target > current {
                    flows.targets.push((village_id.clone(), target - current));
                }
            }
            flows
        }
    }
}

/// Estimated cost in USD of moving `flows`, given today's villages in
/// `state`.
///
/// A source village without a quote today (stale, or no longer active) is
/// costed as a plain withdrawal: gas only, money arrives as USDC.
///
/// # Errors
/// `NoSwapRoute` when a needed swap cannot be done.
pub fn estimate(flows: &Flows, state: &MarketState) -> Result<Decimal, NoSwapRoute> {
    let mut cost = Decimal::ZERO;

    // ① Leaving each source village (cash leaves for free).
    for (source, amount) in &flows.sources {
        if let Source::Village(village_id) = source {
            cost += step_cost(find(state, village_id), *amount);
        }
    }
    // ③ Entering each target village.
    for (village_id, amount) in &flows.targets {
        cost += step_cost(find(state, village_id), *amount);
    }
    // ② Swaps: walk sources and targets together and cost every piece of
    // money that arrives in a token its target does not accept.
    let mut source_left: Vec<Decimal> = flows.sources.iter().map(|(_, a)| *a).collect();
    let mut target_left: Vec<Decimal> = flows.targets.iter().map(|(_, a)| *a).collect();
    let (mut s, mut t) = (0, 0);
    while s < source_left.len() && t < target_left.len() {
        let piece = source_left[s].min(target_left[t]);
        if piece > Decimal::ZERO {
            let target = find(state, &flows.targets[t].0);
            let held = token_in_hand(&flows.sources[s].0, target, state);
            if !accepts(target, &held) {
                cost += swap_cost(piece, &held, target, state)?;
            }
        }
        source_left[s] -= piece;
        target_left[t] -= piece;
        if source_left[s].is_zero() {
            s += 1;
        }
        if target_left[t].is_zero() {
            t += 1;
        }
    }
    Ok(cost)
}

fn find<'a>(state: &'a MarketState, village_id: &str) -> Option<&'a VillageQuote> {
    state
        .villages
        .iter()
        .find(|quote| quote.village_id == village_id)
}

/// Cost of one exit or entry of `amount` in `village`: pool fee, slippage
/// for LP pools (lending has no price impact), and one transaction of gas.
/// Unknown village: gas only.
fn step_cost(village: Option<&VillageQuote>, amount: Decimal) -> Decimal {
    let Some(village) = village else {
        return GAS_USD_PER_TX;
    };
    let fee = fee(amount, village.fee_bps);
    let slip = if village.kind == "stable_lp" {
        slippage(amount, village.tvl_usd)
    } else {
        Decimal::ZERO
    };
    fee + slip + GAS_USD_PER_TX
}

/// The token money has after leaving `source`. A pool that holds several
/// tokens pays out one the target accepts when it can, otherwise USDC.
fn token_in_hand(source: &Source, target: Option<&VillageQuote>, state: &MarketState) -> String {
    let Source::Village(village_id) = source else {
        return CASH_TOKEN.to_string();
    };
    let Some(source) = find(state, village_id) else {
        return CASH_TOKEN.to_string(); // unknown source: plain withdrawal as USDC
    };
    if let Some(token) = source.tokens.iter().find(|token| accepts(target, token)) {
        return token.clone();
    }
    if source.tokens.iter().any(|token| token == CASH_TOKEN) {
        return CASH_TOKEN.to_string();
    }
    source
        .tokens
        .first()
        .cloned()
        .unwrap_or_else(|| CASH_TOKEN.to_string())
}

/// An unknown target accepts anything: the guardrails refuse unknown targets
/// before costs are estimated, so this only avoids inventing a swap.
fn accepts(village: Option<&VillageQuote>, token: &str) -> bool {
    village.is_none_or(|village| village.tokens.iter().any(|t| t == token))
}

/// Swapping `amount` of `held` into a token `target` accepts, through the
/// swap pool.
fn swap_cost(
    amount: Decimal,
    held: &str,
    target: Option<&VillageQuote>,
    state: &MarketState,
) -> Result<Decimal, NoSwapRoute> {
    let pool = find(state, SWAP_VILLAGE_ID).ok_or(NoSwapRoute)?;
    let trades = |token: &str| pool.tokens.iter().any(|t| t == token);
    let target_tokens = target.map(|t| t.tokens.as_slice()).unwrap_or_default();
    if !trades(held) || !target_tokens.iter().any(|token| trades(token)) {
        return Err(NoSwapRoute);
    }
    Ok(fee(amount, pool.fee_bps) + slippage(amount, pool.tvl_usd) + GAS_USD_PER_TX)
}

fn fee(amount: Decimal, fee_bps: u32) -> Decimal {
    amount * Decimal::from(fee_bps) / Decimal::from(BPS_PER_UNIT)
}

/// `amount × k × amount / TVL`, never more than the amount itself. A pool
/// with no TVL would take everything, so its cost is the whole amount.
fn slippage(amount: Decimal, tvl_usd: Decimal) -> Decimal {
    if tvl_usd <= Decimal::ZERO {
        return amount;
    }
    (amount * SLIPPAGE_K * amount / tvl_usd).min(amount)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::NaiveDate;

    use super::*;
    use crate::sim::ledger::Ledger;

    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    fn quote(id: &str, kind: &str, tvl: &str, fee_bps: u32, tokens: &[&str]) -> VillageQuote {
        VillageQuote {
            village_id: id.to_string(),
            kind: kind.to_string(),
            apy: d("10"),
            tvl_usd: d(tvl),
            fee_bps,
            tokens: tokens.iter().map(|t| t.to_string()).collect(),
        }
    }

    /// The 4 launch villages with the fees and tokens of migration 0006 and
    /// the TVLs used in the Day 4 plan examples.
    fn launch_villages() -> Vec<VillageQuote> {
        vec![
            quote(
                "rhea-dex-frax-usdc",
                "stable_lp",
                "11551",
                2,
                &["FRAX", "USDC"],
            ),
            quote(
                "rhea-dex-stable4",
                "stable_lp",
                "104600",
                2,
                &["USDT", "USDC", "USDT.e", "USDC.e"],
            ),
            quote("rhea-lend-usdc", "lending", "2300000", 0, &["USDC"]),
            quote("rhea-lend-usdt", "lending", "2000000", 0, &["USDT"]),
        ]
    }

    fn state(villages: Vec<VillageQuote>, cash: &str, positions: &[(&str, &str)]) -> MarketState {
        MarketState {
            day: NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(),
            villages,
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
            weights: pairs.iter().map(|(id, w)| (id.to_string(), *w)).collect(),
            reason: "test".to_string(),
        }
    }

    fn cost_of(action: &Action, s: &MarketState) -> Result<Decimal, NoSwapRoute> {
        estimate(&flows_of(action, s), s)
    }

    /// `amount × 0.1 × amount / tvl`, the slippage formula.
    fn slip(amount: &str, tvl: &str) -> Decimal {
        d(amount) * d("0.1") * d(amount) / d(tvl)
    }

    #[test]
    fn hold_and_say_cost_nothing() {
        let s = state(launch_villages(), "1000", &[]);
        let say = Action::Say {
            text: "hi".to_string(),
            reason: "test".to_string(),
        };

        assert_eq!(
            cost_of(&Action::Hold { reason: "x".into() }, &s),
            Ok(Decimal::ZERO)
        );
        assert_eq!(cost_of(&say, &s), Ok(Decimal::ZERO));
    }

    #[test]
    fn plan_example_1_lending_usdt_to_lending_usdc_needs_a_swap() {
        // Exit gas 0.02 + swap (0.20 fee + slippage + 0.02 gas) + enter gas 0.02.
        let s = state(launch_villages(), "0", &[("rhea-lend-usdt", "1000")]);

        let cost = cost_of(&move_action("rhea-lend-usdt", "rhea-lend-usdc", 100), &s).unwrap();

        assert_eq!(cost, d("0.26") + slip("1000", "104600"));
        assert_eq!(cost.round_dp(2), d("1.22"));
    }

    #[test]
    fn lending_usdc_to_the_stable_pool_needs_no_swap() {
        // The stable pool accepts USDC: exit gas + enter (fee + slippage + gas).
        let s = state(launch_villages(), "0", &[("rhea-lend-usdc", "1000")]);

        let cost = cost_of(&move_action("rhea-lend-usdc", "rhea-dex-stable4", 100), &s).unwrap();

        assert_eq!(cost, d("0.24") + slip("1000", "104600"));
    }

    #[test]
    fn plan_example_2_cash_into_the_tiny_frax_pool_is_very_expensive() {
        let s = state(launch_villages(), "10000", &[]);

        let cost = cost_of(&rebalance(&[("rhea-dex-frax-usdc", 100)]), &s).unwrap();

        // 2 bps fee (2.00) + slippage (about 865.73) + gas; no swap (USDC accepted).
        assert_eq!(cost, d("2.02") + slip("10000", "11551"));
        assert_eq!(cost.round(), d("868"));
    }

    #[test]
    fn leaving_an_lp_pays_its_fee_and_slippage() {
        // Out of FRAX-USDC in USDC (accepted by lend-usdc): no swap.
        let s = state(launch_villages(), "0", &[("rhea-dex-frax-usdc", "1000")]);

        let cost = cost_of(
            &move_action("rhea-dex-frax-usdc", "rhea-lend-usdc", 100),
            &s,
        )
        .unwrap();

        assert_eq!(cost, d("0.24") + slip("1000", "11551"));
    }

    #[test]
    fn leaving_frax_usdc_for_usdt_lending_swaps_usdc_into_usdt() {
        let s = state(launch_villages(), "0", &[("rhea-dex-frax-usdc", "1000")]);

        let cost = cost_of(
            &move_action("rhea-dex-frax-usdc", "rhea-lend-usdt", 100),
            &s,
        )
        .unwrap();

        // Exit LP (0.20 + slip + 0.02) + swap (0.20 + slip + 0.02) + enter gas 0.02.
        let expected = d("0.46") + slip("1000", "11551") + slip("1000", "104600");
        assert_eq!(cost, expected);
    }

    #[test]
    fn a_needed_swap_without_a_fresh_swap_pool_has_no_route() {
        let without_swap_pool: Vec<VillageQuote> = launch_villages()
            .into_iter()
            .filter(|q| q.village_id != SWAP_VILLAGE_ID)
            .collect();
        let s = state(without_swap_pool, "0", &[("rhea-lend-usdt", "1000")]);

        assert_eq!(
            cost_of(&move_action("rhea-lend-usdt", "rhea-lend-usdc", 50), &s),
            Err(NoSwapRoute)
        );
        // No swap needed: the missing swap pool does not matter.
        let s = state(
            launch_villages()
                .into_iter()
                .filter(|q| q.village_id != SWAP_VILLAGE_ID)
                .collect(),
            "1000",
            &[],
        );
        assert!(cost_of(&rebalance(&[("rhea-lend-usdc", 100)]), &s).is_ok());
    }

    #[test]
    fn rebalance_costs_each_village_once_and_swaps_per_piece() {
        // Equity 2000: cash 1000 + lend-usdt 1000 -> 50% stable4, 50% lend-usdc.
        // Pairing (cash first, targets by id): cash -> stable4 (USDC accepted),
        // lend-usdt -> lend-usdc (USDT must be swapped).
        let s = state(launch_villages(), "1000", &[("rhea-lend-usdt", "1000")]);
        let action = rebalance(&[("rhea-dex-stable4", 50), ("rhea-lend-usdc", 50)]);

        let flows = flows_of(&action, &s);
        let cost = estimate(&flows, &s).unwrap();

        assert_eq!(flows.moved(), d("2000"));
        // exit lend-usdt gas 0.02; enter stable4 0.20 + slip + 0.02; enter lend-usdc 0.02;
        // swap 0.20 + slip + 0.02.
        let expected = d("0.48") + slip("1000", "104600") + slip("1000", "104600");
        assert_eq!(cost, expected);
    }

    #[test]
    fn rebalance_flows_only_count_what_changes() {
        // 600 / 400 -> 30 / 70 moves 300 from lend-usdc to lend-usdt.
        let s = state(
            launch_villages(),
            "0",
            &[("rhea-lend-usdc", "600"), ("rhea-lend-usdt", "400")],
        );

        let flows = flows_of(
            &rebalance(&[("rhea-lend-usdc", 30), ("rhea-lend-usdt", 70)]),
            &s,
        );

        assert_eq!(
            flows.sources,
            vec![(Source::Village("rhea-lend-usdc".into()), d("300"))]
        );
        assert_eq!(flows.targets, vec![("rhea-lend-usdt".into(), d("300"))]);
    }

    #[test]
    fn a_stale_source_is_costed_as_a_plain_usdc_withdrawal() {
        // "old" has no quote today: exit gas only, money arrives as USDC.
        let s = state(launch_villages(), "0", &[("old", "1000")]);

        let cost = cost_of(&move_action("old", "rhea-lend-usdc", 100), &s).unwrap();

        assert_eq!(cost, d("0.04"));
    }

    #[test]
    fn slippage_grows_with_size_and_a_pool_without_tvl_takes_everything() {
        assert!(slippage(d("2000"), d("100000")) > d("2") * slippage(d("1000"), d("100000")));
        assert_eq!(slippage(d("50"), Decimal::ZERO), d("50"));
        // Never more than the amount itself.
        assert_eq!(slippage(d("1000000"), d("1")), d("1000000"));
    }

    /// F6 acceptance (SPEC F6): with the same APY everywhere, an avatar that
    /// moves every day ends with less than one that holds.
    #[test]
    fn moving_often_ends_below_holding_with_the_same_yield() {
        let villages = vec![
            quote(
                "rhea-dex-stable4",
                "stable_lp",
                "104600",
                2,
                &["USDT", "USDC"],
            ),
            quote("rhea-lend-usdc", "lending", "2300000", 0, &["USDC"]),
        ];
        let apys: BTreeMap<String, Decimal> = villages
            .iter()
            .map(|q| (q.village_id.clone(), d("10")))
            .collect();
        let mut holder = Ledger::new(d("10000"));
        let mut mover = Ledger::new(d("10000"));
        let into_lend = rebalance(&[("rhea-lend-usdc", 100)]);
        for ledger in [&mut holder, &mut mover] {
            let s = state(villages.clone(), "10000", &[]);
            let cost = cost_of(&into_lend, &s).unwrap();
            ledger.apply_action(&into_lend, cost).unwrap();
        }

        for day in 0..30 {
            // The mover flips between the two villages every day.
            let (from, to) = if day % 2 == 0 {
                ("rhea-lend-usdc", "rhea-dex-stable4")
            } else {
                ("rhea-dex-stable4", "rhea-lend-usdc")
            };
            let s = MarketState {
                day: NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(),
                villages: villages.clone(),
                cash_usd: mover.cash_usd(),
                positions: mover.positions().clone(),
            };
            let flip = move_action(from, to, 100);
            let cost = cost_of(&flip, &s).unwrap();
            mover.apply_action(&flip, cost).unwrap();

            holder.accrue_one_day(&apys).unwrap();
            mover.accrue_one_day(&apys).unwrap();
        }

        assert!(
            mover.equity() < holder.equity(),
            "mover {} vs holder {}",
            mover.equity(),
            holder.equity()
        );
    }
}
