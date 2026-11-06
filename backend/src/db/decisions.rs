//! `decisions`: one row per agent decision (F4 acceptance: input hash, action,
//! validity and reason are logged for every decision).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// A decision to store. `valid` is derived from `rejected_rule` (the table
/// has `CHECK (valid = (rejected_rule IS NULL))`), so the two cannot disagree.
#[derive(Debug, Clone, PartialEq)]
pub struct NewDecision {
    pub avatar_id: Uuid,
    pub ts: DateTime<Utc>,
    /// `backtest`, `sim` or `real`.
    pub source: String,
    pub input_hash: String,
    pub model: String,
    /// `None` when the LLM gave no usable answer.
    pub proposed_action: Option<Value>,
    pub applied_action: Value,
    /// `None` when the proposal was applied as is.
    pub rejected_rule: Option<String>,
    /// Public, at most 140 characters (DB CHECK).
    pub reason_public: Option<String>,
    pub cost_usd: Decimal,
    /// Exact LLM / parse error per attempt; internal only, at most 8000
    /// characters (DB CHECK, migration 0009).
    pub error_detail: Option<String>,
}

/// A stored decision, as read back.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionRow {
    pub id: i64,
    pub avatar_id: Uuid,
    pub ts: DateTime<Utc>,
    pub source: String,
    pub input_hash: String,
    pub model: String,
    pub proposed_action: Option<Value>,
    pub applied_action: Value,
    pub valid: bool,
    pub rejected_rule: Option<String>,
    pub reason_public: Option<String>,
    pub cost_usd: Decimal,
    /// Internal only: the API's `DecisionDto` deliberately leaves it out.
    pub error_detail: Option<String>,
}

/// Inserts one decision and returns its id.
///
/// Takes any executor (`&PgPool`, or `&mut *tx` inside the live tick's
/// transaction).
pub async fn insert_decision<'e>(
    executor: impl PgExecutor<'e>,
    decision: &NewDecision,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO decisions (avatar_id, ts, source, input_hash, model, proposed_action,
                               applied_action, valid, rejected_rule, reason_public, cost_usd,
                               error_detail)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING id
        "#,
        decision.avatar_id,
        decision.ts,
        decision.source,
        decision.input_hash,
        decision.model,
        decision.proposed_action,
        decision.applied_action,
        decision.rejected_rule.is_none(),
        decision.rejected_rule,
        decision.reason_public,
        decision.cost_usd,
        decision.error_detail,
    )
    .fetch_one(executor)
    .await
}

/// Ticks since the avatar's last applied MOVE or REBALANCE for `source`, for
/// the cooldown rule. `None` when it never moved.
///
/// Counted in decisions: the move itself is tick 0, so a move followed by two
/// more decisions gives 3 at the next decision.
pub async fn ticks_since_last_move(
    pool: &PgPool,
    avatar_id: Uuid,
    source: &str,
) -> sqlx::Result<Option<u32>> {
    let row = sqlx::query!(
        r#"
        WITH last_move AS (
            SELECT max(id) AS id FROM decisions
            WHERE avatar_id = $1 AND source = $2
              AND applied_action->>'action' IN ('MOVE', 'REBALANCE')
        )
        SELECT (SELECT id FROM last_move) AS last_move_id,
               (SELECT count(*) FROM decisions
                WHERE avatar_id = $1 AND source = $2
                  AND id > (SELECT id FROM last_move)) AS "after!"
        "#,
        avatar_id,
        source,
    )
    .fetch_one(pool)
    .await?;

    Ok(row.last_move_id.map(|_| {
        u32::try_from(row.after)
            .unwrap_or(u32::MAX)
            .saturating_add(1)
    }))
}

/// Deletes the avatar's `backtest` decisions before a new backtest stores its
/// own (D63). `sim` and `real` decisions are never touched.
pub async fn delete_backtest_decisions<'e>(
    executor: impl PgExecutor<'e>,
    avatar_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query!(
        "DELETE FROM decisions WHERE avatar_id = $1 AND source = 'backtest'",
        avatar_id,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// The avatar's decisions for `source`, newest first.
pub async fn list_decisions(
    pool: &PgPool,
    avatar_id: Uuid,
    source: &str,
    limit: i64,
) -> sqlx::Result<Vec<DecisionRow>> {
    sqlx::query_as!(
        DecisionRow,
        r#"
        SELECT id, avatar_id, ts, source, input_hash, model, proposed_action, applied_action,
               valid, rejected_rule, reason_public, cost_usd, error_detail
        FROM decisions
        WHERE avatar_id = $1 AND source = $2
        ORDER BY ts DESC, id DESC
        LIMIT $3
        "#,
        avatar_id,
        source,
        limit,
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::avatars;
    use serde_json::json;

    async fn create_avatar(pool: &PgPool) -> Uuid {
        let owner = "alice.testnet";
        avatars::ensure_user(pool, owner).await.unwrap();
        let config_id = avatars::insert_config(
            pool,
            &avatars::NewConfig {
                persona: "Prefer lending.".to_string(),
                risk: "low".to_string(),
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
                starting_capital_usd: Decimal::from(1000),
                active: true,
            },
        )
        .await
        .unwrap()
    }

    fn decision(avatar_id: Uuid, action: &str, rejected_rule: Option<&str>) -> NewDecision {
        NewDecision {
            avatar_id,
            ts: Utc::now(),
            source: "sim".to_string(),
            input_hash: "abc".to_string(),
            model: "test-model".to_string(),
            proposed_action: Some(json!({"action": action, "reason": "r"})),
            applied_action: json!({"action": action, "reason": "r"}),
            rejected_rule: rejected_rule.map(str::to_string),
            reason_public: Some("r".to_string()),
            cost_usd: Decimal::ZERO,
            error_detail: None,
        }
    }

    #[sqlx::test]
    async fn decision_is_stored_and_valid_follows_the_rejected_rule(pool: PgPool) {
        let avatar = create_avatar(&pool).await;

        insert_decision(&pool, &decision(avatar, "HOLD", None))
            .await
            .unwrap();
        let mut refused = decision(avatar, "HOLD", Some("max_share"));
        refused.proposed_action = None;
        insert_decision(&pool, &refused).await.unwrap();

        let rows = list_decisions(&pool, avatar, "sim", 10).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].valid);
        assert_eq!(rows[0].rejected_rule.as_deref(), Some("max_share"));
        assert_eq!(rows[0].proposed_action, None);
        assert!(rows[1].valid);
    }

    #[sqlx::test]
    async fn reason_longer_than_140_characters_is_refused_by_the_database(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        let mut long = decision(avatar, "HOLD", None);
        long.reason_public = Some("a".repeat(141));

        assert!(insert_decision(&pool, &long).await.is_err());
    }

    #[sqlx::test]
    async fn error_detail_is_stored_up_to_8000_characters(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        let mut failed = decision(avatar, "HOLD", Some("llm_error"));
        failed.error_detail = Some("a".repeat(8000));
        insert_decision(&pool, &failed).await.unwrap();

        let rows = list_decisions(&pool, avatar, "sim", 10).await.unwrap();
        assert_eq!(rows[0].error_detail.as_deref().map(str::len), Some(8000));

        let mut too_long = decision(avatar, "HOLD", Some("llm_error"));
        too_long.error_detail = Some("a".repeat(8001));
        assert!(insert_decision(&pool, &too_long).await.is_err());
    }

    #[sqlx::test]
    async fn ticks_since_last_move_counts_decisions_after_the_move(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        assert_eq!(
            ticks_since_last_move(&pool, avatar, "sim").await.unwrap(),
            None
        );

        insert_decision(&pool, &decision(avatar, "HOLD", None))
            .await
            .unwrap();
        assert_eq!(
            ticks_since_last_move(&pool, avatar, "sim").await.unwrap(),
            None
        );

        insert_decision(&pool, &decision(avatar, "REBALANCE", None))
            .await
            .unwrap();
        assert_eq!(
            ticks_since_last_move(&pool, avatar, "sim").await.unwrap(),
            Some(1)
        );

        insert_decision(&pool, &decision(avatar, "SAY", None))
            .await
            .unwrap();
        insert_decision(&pool, &decision(avatar, "HOLD", None))
            .await
            .unwrap();
        assert_eq!(
            ticks_since_last_move(&pool, avatar, "sim").await.unwrap(),
            Some(3)
        );

        // Other sources are counted separately.
        assert_eq!(
            ticks_since_last_move(&pool, avatar, "backtest")
                .await
                .unwrap(),
            None
        );
    }
}
