//! `decide`: one decision for one avatar (TECH §5.4 steps 2 to 5).
//!
//! ```text
//! prompt ─> cache? ─> LLM ─> parse ─(fail)─> LLM again ─> parse ─(fail)─> HOLD "parse_error"
//!                                │                          │
//!                                └──────── ok ──────────────┴─> guardrails ─(refused)─> HOLD "<rule>"
//!                                                                    └─ ok ─> action
//! ```
//!
//! "The LLM proposes, code decides": whatever happens (network error, bad
//! JSON, a rule broken), the result is always an action we can apply, and the
//! `Decision` says why.

use anyhow::Result;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use super::guardrails::{self, Risk};
use super::llm::LlmClient;
use super::prompt::{self, Profile};
use super::{Action, MarketState};
use crate::db::llm_cache;

/// `rejected_rule` when the model never gave a parseable action.
pub const PARSE_ERROR: &str = "parse_error";
/// `rejected_rule` when the LLM could not be reached (network, timeout, API error).
pub const LLM_ERROR: &str = "llm_error";

/// LLM calls per decision: the first one plus one retry (SPEC F4, CLAUDE.md).
const MAX_ATTEMPTS: usize = 2;

/// Most characters kept in `Decision::error_detail`. Same limit as the DB
/// CHECK on `decisions.error_detail` (migration 0009).
pub const MAX_ERROR_DETAIL_CHARS: usize = 8000;

/// Why the model gave no usable action after all attempts.
#[derive(Debug, Clone, PartialEq)]
struct ModelFailure {
    /// `LLM_ERROR` or `PARSE_ERROR`, from the last attempt.
    rule: &'static str,
    /// One line per attempt, cut to `MAX_ERROR_DETAIL_CHARS`.
    detail: String,
}

/// The outcome of one decision, ready to be stored in `decisions`.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    /// sha256 of the market state text sent to the model.
    pub input_hash: String,
    pub model: String,
    /// What the model asked for; `None` when it gave no usable answer.
    pub proposed: Option<Action>,
    /// What code allows: the proposal, or HOLD.
    pub applied: Action,
    /// `None` when the proposal was applied as is.
    pub rejected_rule: Option<String>,
    /// Estimated cost of `applied` in USD (`sim::costs`); 0 for HOLD and SAY.
    pub cost_usd: Decimal,
    /// The exact error of each attempt when the model gave no usable action
    /// (`llm_error` / `parse_error`); `None` otherwise. Internal only: stored
    /// in the DB, never sent to the API or `/ws`. Never holds the model's text.
    pub error_detail: Option<String>,
}

impl Decision {
    /// The public reason shown in the UI: the applied action's reason, cut to
    /// 140 characters. Never contains the persona text we wrote; the model's
    /// own reason is public by design (SPEC F4.3).
    pub fn reason_public(&self) -> String {
        action_reason(&self.applied)
            .chars()
            .take(guardrails::MAX_REASON_CHARS)
            .collect()
    }
}

/// Asks the model for an action and checks it.
///
/// Answers are cached by (model, system prompt, user prompt): the same input
/// returns the same answer without a new call. Only parseable answers are
/// cached, so a bad answer is retried next time.
///
/// Cache read/write errors are logged and ignored: a cache problem should not
/// turn every decision into HOLD.
pub async fn decide(
    llm: &LlmClient,
    pool: &PgPool,
    profile: &Profile<'_>,
    state: &MarketState,
    ticks_since_last_move: Option<u32>,
) -> Decision {
    let prompt = prompt::build(profile, state, ticks_since_last_move);
    let cache_key = sha256_hex(&[llm.model(), &prompt.system, &prompt.user]);

    // The detail is split off here so `check_proposal` stays a small pure
    // step that only needs the rule name.
    let (proposal, error_detail) = match cached_action(pool, &cache_key).await {
        Some(action) => (Ok(action), None),
        None => match ask_model(llm, pool, &cache_key, &prompt).await {
            Ok(action) => (Ok(action), None),
            Err(failure) => (Err(failure.rule), Some(failure.detail)),
        },
    };

    let (proposed, applied, rejected_rule, cost_usd) =
        check_proposal(proposal, state, profile.risk, ticks_since_last_move);
    Decision {
        input_hash: sha256_hex(&[&prompt.user]),
        model: llm.model().to_string(),
        proposed,
        applied,
        rejected_rule,
        cost_usd,
        error_detail,
    }
}

async fn cached_action(pool: &PgPool, cache_key: &str) -> Option<Action> {
    match llm_cache::get(pool, cache_key).await {
        Ok(Some(response)) => response
            .get("text")
            .and_then(Value::as_str)
            .and_then(|text| parse_action(text).ok()),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!("llm cache read failed, calling the model: {err}");
            None
        }
    }
}

/// Calls the model up to `MAX_ATTEMPTS` times until it returns a parseable
/// action. Otherwise returns the rule name of the last failure and the error
/// of every attempt.
async fn ask_model(
    llm: &LlmClient,
    pool: &PgPool,
    cache_key: &str,
    prompt: &prompt::Prompt,
) -> Result<Action, ModelFailure> {
    let mut last_failure = LLM_ERROR;
    let mut attempt_errors: Vec<String> = Vec::with_capacity(MAX_ATTEMPTS);
    for attempt in 1..=MAX_ATTEMPTS {
        let text = match llm.complete(&prompt.system, &prompt.user).await {
            Ok(text) => text,
            Err(err) => {
                // `{:#}` prints the cause chain (e.g. the provider's 429
                // message); it never contains the prompt.
                tracing::warn!(attempt, "LLM call failed: {err:#}");
                attempt_errors.push(format!("attempt {attempt}: LLM call failed: {err:#}"));
                last_failure = LLM_ERROR;
                continue;
            }
        };
        match parse_action(&text) {
            Ok(action) => {
                if let Err(err) = llm_cache::put(pool, cache_key, &json!({ "text": text })).await {
                    tracing::warn!("llm cache write failed: {err}");
                }
                return Ok(action);
            }
            Err(err) => {
                // `err` describes the problem without the model text (see
                // `parse_action`), so the persona cannot leak into the row.
                tracing::warn!(attempt, "LLM answer is not a valid action: {err}");
                attempt_errors.push(format!(
                    "attempt {attempt}: LLM answer is not a valid action: {err}"
                ));
                last_failure = PARSE_ERROR;
            }
        }
    }
    Err(ModelFailure {
        rule: last_failure,
        detail: cut_chars(&attempt_errors.join("\n"), MAX_ERROR_DETAIL_CHARS),
    })
}

/// The first `max` characters of `text`. Counts characters, not bytes, like
/// Postgres `char_length`, and never splits a multi-byte character.
fn cut_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Pure last step: runs the guardrails on a proposal and returns
/// `(proposed, applied, rejected_rule, cost_usd)`. A failure becomes HOLD
/// with no cost.
pub fn check_proposal(
    proposal: Result<Action, &'static str>,
    state: &MarketState,
    risk: Risk,
    ticks_since_last_move: Option<u32>,
) -> (Option<Action>, Action, Option<String>, Decimal) {
    match proposal {
        Err(failure) => (
            None,
            hold(failure),
            Some(failure.to_string()),
            Decimal::ZERO,
        ),
        Ok(action) => match guardrails::check(&action, state, risk, ticks_since_last_move) {
            Ok(cost_usd) => (Some(action.clone()), action, None, cost_usd),
            Err(rule) => {
                let rule = rule.as_str();
                (
                    Some(action),
                    hold(rule),
                    Some(rule.to_string()),
                    Decimal::ZERO,
                )
            }
        },
    }
}

/// HOLD with a short public reason naming the rule.
fn hold(rule: &str) -> Action {
    Action::Hold {
        reason: format!("Holding: proposal refused ({rule})."),
    }
}

fn action_reason(action: &Action) -> &str {
    match action {
        Action::Hold { reason }
        | Action::Move { reason, .. }
        | Action::Rebalance { reason, .. }
        | Action::Say { reason, .. } => reason,
    }
}

/// Reads one action from the model's text.
///
/// Models often wrap JSON in a markdown fence or add a sentence around it, so
/// this takes the text from the first `{` to the last `}`. Anything that is
/// not exactly one of the four actions fails (the enum is strict).
///
/// # Errors
/// A short description, without the model text (it may echo the persona).
pub fn parse_action(text: &str) -> Result<Action, String> {
    let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) else {
        return Err("no JSON object in the answer".to_string());
    };
    if end < start {
        return Err("no JSON object in the answer".to_string());
    }
    serde_json::from_str(&text[start..=end]).map_err(|err| format!("invalid action JSON: {err}"))
}

/// Hex sha256 of the parts, each followed by a 0 byte so ("ab","c") and
/// ("a","bc") give different hashes.
fn sha256_hex(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    use super::*;
    use crate::agent::VillageQuote;

    fn state() -> MarketState {
        MarketState {
            day: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            villages: vec![
                VillageQuote {
                    village_id: "a".to_string(),
                    kind: "lending".to_string(),
                    apy: Decimal::TEN,
                    tvl_usd: Decimal::from(1_000_000),
                    fee_bps: 0,
                    tokens: vec!["USDC".to_string()],
                },
                VillageQuote {
                    village_id: "b".to_string(),
                    kind: "lending".to_string(),
                    apy: Decimal::from(12),
                    tvl_usd: Decimal::from(1_000_000),
                    fee_bps: 0,
                    tokens: vec!["USDC".to_string()],
                },
            ],
            cash_usd: Decimal::from(1000),
            positions: BTreeMap::new(),
        }
    }

    #[test]
    fn parses_plain_json() {
        let action = parse_action(r#"{"action":"HOLD","reason":"calm market"}"#).unwrap();
        assert_eq!(
            action,
            Action::Hold {
                reason: "calm market".to_string()
            }
        );
    }

    #[test]
    fn parses_json_inside_a_markdown_fence_or_a_sentence() {
        let fenced = "```json\n{\"action\":\"HOLD\",\"reason\":\"x\"}\n```";
        let chatty =
            "Sure! Here is my decision: {\"action\":\"HOLD\",\"reason\":\"x\"} Hope it helps.";

        assert!(parse_action(fenced).is_ok());
        assert!(parse_action(chatty).is_ok());
    }

    #[test]
    fn rejects_text_without_a_valid_action() {
        assert!(parse_action("I would hold.").is_err());
        assert!(parse_action("} {").is_err());
        assert!(parse_action(r#"{"action":"BUY","reason":"x"}"#).is_err());
        assert!(parse_action(r#"{"action":"MOVE","from":"a","to":"b","reason":"x"}"#).is_err());
    }

    #[test]
    fn parse_error_message_does_not_echo_the_model_text() {
        let err = parse_action("my secret strategy {not json}").unwrap_err();
        assert!(!err.contains("secret"));
    }

    #[test]
    fn allowed_proposal_is_applied() {
        let rebalance = Action::Rebalance {
            weights: BTreeMap::from([("a".to_string(), 50), ("b".to_string(), 50)]),
            reason: "split".to_string(),
        };

        let (proposed, applied, rule, cost) =
            check_proposal(Ok(rebalance.clone()), &state(), Risk::Low, None);

        assert_eq!(proposed, Some(rebalance.clone()));
        assert_eq!(applied, rebalance);
        assert_eq!(rule, None);
        // Cash into two USDC lending villages: 2 × 0.02 of gas.
        assert_eq!(cost, Decimal::new(4, 2));
    }

    #[test]
    fn refused_proposal_becomes_hold_and_keeps_the_rule() {
        let all_in = Action::Rebalance {
            weights: BTreeMap::from([("b".to_string(), 100)]),
            reason: "all in".to_string(),
        };

        let (proposed, applied, rule, cost) =
            check_proposal(Ok(all_in.clone()), &state(), Risk::Low, None);

        assert_eq!(proposed, Some(all_in));
        assert!(matches!(applied, Action::Hold { .. }));
        assert_eq!(rule.as_deref(), Some("max_share"));
        assert_eq!(cost, Decimal::ZERO);
    }

    #[test]
    fn no_usable_answer_becomes_hold_without_a_proposal() {
        let (proposed, applied, rule, cost) =
            check_proposal(Err(PARSE_ERROR), &state(), Risk::Low, None);

        assert_eq!(proposed, None);
        assert!(matches!(applied, Action::Hold { .. }));
        assert_eq!(rule.as_deref(), Some("parse_error"));
        assert_eq!(cost, Decimal::ZERO);
    }

    #[test]
    fn error_detail_is_cut_to_the_database_limit_by_characters() {
        // "é" is 2 bytes: cutting by bytes would split it or keep too few.
        let long = "é".repeat(MAX_ERROR_DETAIL_CHARS + 10);

        let cut = cut_chars(&long, MAX_ERROR_DETAIL_CHARS);

        assert_eq!(cut.chars().count(), MAX_ERROR_DETAIL_CHARS);
        assert_eq!(cut_chars("short", MAX_ERROR_DETAIL_CHARS), "short");
    }

    #[test]
    fn hash_separates_its_parts() {
        assert_ne!(sha256_hex(&["ab", "c"]), sha256_hex(&["a", "bc"]));
        assert_eq!(sha256_hex(&["x"]).len(), 64);
    }
}
