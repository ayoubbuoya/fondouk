//! Database access for progression (D74): the activity counts XP is made of,
//! and the `avatar_milestones` table.
//!
//! Counts read live rows only (`sim` and `real`): a backtest never gives XP.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

use crate::progression::{ActivityCounts, NON_GUARDRAIL_RULES};

/// One stored milestone of a character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneRow {
    /// `level:4` or `badge:first_steps`.
    pub milestone: String,
    pub reached_at: DateTime<Utc>,
}

/// A milestone with the public info of its character (Hall of Fame feed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentMilestoneRow {
    pub avatar_id: Uuid,
    pub avatar_name: String,
    pub sprite: String,
    pub milestone: String,
    pub reached_at: DateTime<Utc>,
}

/// The activity counts of one character (`Some(id)`) or of every character
/// (`None`), by avatar id. A character with no decision yet is in the map
/// with zero counts.
///
/// One query for any number of characters: three grouped sub-selects joined
/// to `avatars`, so `GET /avatars` does not run one query per character.
/// It reads all live `decisions` rows; fine at hackathon scale (tens of
/// thousands), a much larger table would want a stored counter.
pub async fn load_counts<'e>(
    executor: impl PgExecutor<'e>,
    avatar_id: Option<Uuid>,
) -> sqlx::Result<HashMap<Uuid, ActivityCounts>> {
    let non_guardrail_rules: Vec<String> = NON_GUARDRAIL_RULES
        .iter()
        .map(|rule| (*rule).to_string())
        .collect();
    let rows = sqlx::query!(
        r#"
        SELECT
            a.id AS "avatar_id!",
            COALESCE(d.valid_ticks, 0) AS "valid_ticks!",
            COALESCE(d.moves, 0) AS "moves!",
            COALESCE(d.messages, 0) AS "messages!",
            COALESCE(d.guardrail_blocks, 0) AS "guardrail_blocks!",
            COALESCE(c.clones, 0) AS "clones!",
            e.first_live_ts AS "first_live_ts?"
        FROM avatars a
        LEFT JOIN (
            SELECT
                avatar_id,
                count(*) FILTER (WHERE valid) AS valid_ticks,
                count(*) FILTER (
                    WHERE valid AND applied_action->>'action' IN ('MOVE', 'REBALANCE')
                ) AS moves,
                count(*) FILTER (
                    WHERE valid AND applied_action->>'action' = 'SAY'
                ) AS messages,
                count(*) FILTER (
                    WHERE NOT valid AND rejected_rule <> ALL($2::TEXT[])
                ) AS guardrail_blocks
            FROM decisions
            WHERE source IN ('sim', 'real')
            GROUP BY avatar_id
        ) d ON d.avatar_id = a.id
        LEFT JOIN (
            SELECT cloned_from, count(*) AS clones
            FROM avatars
            WHERE cloned_from IS NOT NULL
            GROUP BY cloned_from
        ) c ON c.cloned_from = a.id
        LEFT JOIN (
            SELECT avatar_id, min(ts) AS first_live_ts
            FROM equity_snapshots
            WHERE source IN ('sim', 'real')
            GROUP BY avatar_id
        ) e ON e.avatar_id = a.id
        WHERE $1::UUID IS NULL OR a.id = $1
        "#,
        avatar_id,
        &non_guardrail_rules,
    )
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let counts = ActivityCounts {
                valid_ticks: non_negative(row.valid_ticks),
                moves: non_negative(row.moves),
                messages: non_negative(row.messages),
                guardrail_blocks: non_negative(row.guardrail_blocks),
                clones: non_negative(row.clones),
                first_live_ts: row.first_live_ts,
            };
            (row.avatar_id, counts)
        })
        .collect())
}

/// The activity counts of one character; zero counts for an unknown id.
pub async fn counts_for_avatar<'e>(
    executor: impl PgExecutor<'e>,
    avatar_id: Uuid,
) -> sqlx::Result<ActivityCounts> {
    let mut counts = load_counts(executor, Some(avatar_id)).await?;
    Ok(counts.remove(&avatar_id).unwrap_or_default())
}

/// `count(*)` is a BIGINT and never negative; this only changes the type.
fn non_negative(count: i64) -> u64 {
    u64::try_from(count).unwrap_or(0)
}

/// Stores the milestones the character does not have yet and returns only
/// those: the ones to celebrate.
///
/// The primary key `(avatar_id, milestone)` is what makes an unlock happen
/// exactly once: a milestone already stored is skipped by `ON CONFLICT DO
/// NOTHING` and is not in the `RETURNING` list.
pub async fn insert_milestones(
    conn: &mut PgConnection,
    avatar_id: Uuid,
    milestones: &[String],
    reached_at: DateTime<Utc>,
) -> sqlx::Result<Vec<String>> {
    if milestones.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar!(
        r#"
        INSERT INTO avatar_milestones (avatar_id, milestone, reached_at)
        SELECT $1::UUID, t.milestone, $3::TIMESTAMPTZ
        FROM UNNEST($2::TEXT[]) AS t (milestone)
        ON CONFLICT (avatar_id, milestone) DO NOTHING
        RETURNING milestone
        "#,
        avatar_id,
        milestones,
        reached_at,
    )
    .fetch_all(conn)
    .await
}

/// Every milestone of one character, oldest first.
pub async fn list_milestones(pool: &PgPool, avatar_id: Uuid) -> sqlx::Result<Vec<MilestoneRow>> {
    sqlx::query_as!(
        MilestoneRow,
        r#"
        SELECT milestone, reached_at
        FROM avatar_milestones
        WHERE avatar_id = $1
        ORDER BY reached_at, milestone
        "#,
        avatar_id,
    )
    .fetch_all(pool)
    .await
}

/// The latest `limit` milestones of the whole town, newest first.
pub async fn list_recent_milestones(
    pool: &PgPool,
    limit: i64,
) -> sqlx::Result<Vec<RecentMilestoneRow>> {
    sqlx::query_as!(
        RecentMilestoneRow,
        r#"
        SELECT m.avatar_id, a.name AS avatar_name, a.sprite, m.milestone, m.reached_at
        FROM avatar_milestones m
        JOIN avatars a ON a.id = m.avatar_id
        ORDER BY m.reached_at DESC, m.milestone
        LIMIT $1
        "#,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// How many badges each character has (characters with none are absent).
pub async fn badge_counts(pool: &PgPool) -> sqlx::Result<HashMap<Uuid, i64>> {
    let rows = sqlx::query!(
        r#"
        SELECT avatar_id, count(*) AS "badges!"
        FROM avatar_milestones
        WHERE milestone LIKE 'badge:%'
        GROUP BY avatar_id
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.avatar_id, row.badges))
        .collect())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use rust_decimal::Decimal;
    use serde_json::json;

    use super::*;
    use crate::db::avatars::{self, NewAvatar, NewConfig};
    use crate::db::decisions::{self, NewDecision};
    use crate::db::equity::{self, EquityPoint};

    const OWNER: &str = "alice.testnet";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 10, 12, 0, 0).unwrap()
    }

    async fn create_avatar(pool: &PgPool, name: &str) -> Uuid {
        avatars::ensure_user(pool, OWNER).await.unwrap();
        let config_id = avatars::insert_config(
            pool,
            &NewConfig {
                persona: "Lend.".to_string(),
                risk: "low".to_string(),
                goal: "max_yield".to_string(),
                created_by: OWNER.to_string(),
            },
        )
        .await
        .unwrap();
        avatars::insert_avatar(
            pool,
            &NewAvatar {
                owner: OWNER.to_string(),
                config_id,
                name: name.to_string(),
                sprite: "boy".to_string(),
                mode: "sim".to_string(),
                starting_capital_usd: Decimal::from(1000),
                active: true,
                listed: false,
                price_yocto: None,
            },
        )
        .await
        .unwrap()
    }

    /// Stores one decision. `minute` keeps the rows of one avatar apart.
    async fn decide(
        pool: &PgPool,
        avatar: Uuid,
        minute: i64,
        source: &str,
        action: &str,
        rejected_rule: Option<&str>,
    ) {
        decisions::insert_decision(
            pool,
            &NewDecision {
                avatar_id: avatar,
                ts: now() + chrono::Duration::minutes(minute),
                source: source.to_string(),
                input_hash: "0".repeat(64),
                model: "fake-model".to_string(),
                proposed_action: None,
                applied_action: json!({ "action": action }),
                rejected_rule: rejected_rule.map(str::to_string),
                reason_public: None,
                cost_usd: Decimal::ZERO,
                error_detail: None,
            },
        )
        .await
        .unwrap();
    }

    #[sqlx::test]
    async fn counts_split_the_actions_and_ignore_the_backtest(pool: PgPool) {
        let avatar = create_avatar(&pool, "Hanna").await;
        decide(&pool, avatar, 0, "sim", "HOLD", None).await;
        decide(&pool, avatar, 1, "sim", "MOVE", None).await;
        decide(&pool, avatar, 2, "sim", "REBALANCE", None).await;
        decide(&pool, avatar, 3, "sim", "SAY", None).await;
        // Refused by a guardrail: counts as a block, gives no tick.
        decide(&pool, avatar, 4, "sim", "HOLD", Some("max_share")).await;
        // Plumbing failures are not guardrail blocks.
        decide(&pool, avatar, 5, "sim", "HOLD", Some("parse_error")).await;
        decide(&pool, avatar, 6, "sim", "HOLD", Some("llm_error")).await;
        // A backtest never counts, whatever it did.
        decide(&pool, avatar, 7, "backtest", "MOVE", None).await;
        decide(&pool, avatar, 8, "backtest", "HOLD", Some("max_share")).await;
        equity::insert_equity_point(
            &pool,
            avatar,
            "sim",
            &EquityPoint {
                ts: now(),
                equity_usd: Decimal::from(1000),
            },
        )
        .await
        .unwrap();

        let counts = counts_for_avatar(&pool, avatar).await.unwrap();

        assert_eq!(
            counts,
            ActivityCounts {
                valid_ticks: 4,
                moves: 2,
                messages: 1,
                guardrail_blocks: 1,
                clones: 0,
                first_live_ts: Some(now()),
            }
        );
    }

    #[sqlx::test]
    async fn clones_count_for_the_source_and_every_avatar_is_listed(pool: PgPool) {
        let source = create_avatar(&pool, "Source").await;
        let clone = create_avatar(&pool, "Copy").await;
        sqlx::query!(
            "UPDATE avatars SET cloned_from = $1 WHERE id = $2",
            source,
            clone
        )
        .execute(&pool)
        .await
        .unwrap();

        let all = load_counts(&pool, None).await.unwrap();

        assert_eq!(all.len(), 2, "avatars with no decision are listed too");
        assert_eq!(all[&source].clones, 1);
        assert_eq!(all[&clone], ActivityCounts::default());
        assert_eq!(
            counts_for_avatar(&pool, Uuid::nil()).await.unwrap(),
            ActivityCounts::default(),
            "unknown id"
        );
    }

    #[sqlx::test]
    async fn a_milestone_is_stored_and_returned_only_the_first_time(pool: PgPool) {
        let avatar = create_avatar(&pool, "Hanna").await;
        let mut conn = pool.acquire().await.unwrap();
        let first = vec!["badge:first_steps".to_string()];
        let both = vec!["badge:first_steps".to_string(), "level:2".to_string()];
        let later = now() + chrono::Duration::hours(1);

        let unlocked = insert_milestones(&mut conn, avatar, &first, now())
            .await
            .unwrap();
        assert_eq!(unlocked, first);

        // The second call brings one old and one new milestone.
        let unlocked = insert_milestones(&mut conn, avatar, &both, later)
            .await
            .unwrap();
        assert_eq!(unlocked, vec!["level:2".to_string()]);
        let nothing = insert_milestones(&mut conn, avatar, &[], later)
            .await
            .unwrap();
        assert!(nothing.is_empty());

        // The first date is kept: the second call did not overwrite it.
        let stored = list_milestones(&pool, avatar).await.unwrap();
        assert_eq!(
            stored,
            vec![
                MilestoneRow {
                    milestone: "badge:first_steps".to_string(),
                    reached_at: now(),
                },
                MilestoneRow {
                    milestone: "level:2".to_string(),
                    reached_at: later,
                },
            ]
        );
        assert_eq!(badge_counts(&pool).await.unwrap()[&avatar], 1);
        let recent = list_recent_milestones(&pool, 1).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].milestone, "level:2", "newest first");
        assert_eq!(recent[0].avatar_name, "Hanna");
    }

    #[sqlx::test]
    async fn a_malformed_milestone_is_refused_by_the_database(pool: PgPool) {
        let avatar = create_avatar(&pool, "Hanna").await;
        let mut conn = pool.acquire().await.unwrap();

        let result =
            insert_milestones(&mut conn, avatar, &["quest:1".to_string()], now()).await;

        assert!(result.is_err());
    }
}
