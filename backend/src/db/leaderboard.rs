//! Database access for the Leaderboard (F10).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

/// Avatar metadata with aggregated counts for the leaderboard.
#[derive(Debug, Clone)]
pub struct LeaderboardAvatarRow {
    pub id: Uuid,
    pub owner: String,
    pub name: String,
    pub sprite: String,
    pub mode: String,
    pub starting_capital_usd: Decimal,
    pub cloned_from: Option<Uuid>,
    pub listed: bool,
    pub price_yocto: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub clone_count: i64,
    pub moves_count: i64,
    pub first_equity_ts: Option<DateTime<Utc>>,
}

/// An equity snapshot point for leaderboard calculations.
#[derive(Debug, Clone)]
pub struct LeaderboardEquityPoint {
    pub avatar_id: Uuid,
    pub ts: DateTime<Utc>,
    pub equity_usd: Decimal,
}

/// Fetches all avatars with their clone count, moves count, and earliest live equity timestamp.
pub async fn list_leaderboard_avatars(pool: &PgPool) -> sqlx::Result<Vec<LeaderboardAvatarRow>> {
    sqlx::query_as!(
        LeaderboardAvatarRow,
        r#"
        SELECT
            a.id,
            a.owner,
            a.name,
            a.sprite,
            a.mode,
            a.starting_capital_usd,
            a.cloned_from,
            a.listed,
            a.price_yocto,
            a.created_at,
            COALESCE(c.clone_count, 0) AS "clone_count!",
            COALESCE(d.moves_count, 0) AS "moves_count!",
            e.first_equity_ts
        FROM avatars a
        LEFT JOIN (
            SELECT cloned_from, count(*) AS clone_count
            FROM avatars
            WHERE cloned_from IS NOT NULL
            GROUP BY cloned_from
        ) c ON c.cloned_from = a.id
        LEFT JOIN (
            SELECT avatar_id, count(*) AS moves_count
            FROM decisions
            WHERE applied_action->>'action' IN ('MOVE', 'REBALANCE')
            GROUP BY avatar_id
        ) d ON d.avatar_id = a.id
        LEFT JOIN (
            SELECT avatar_id, min(ts) AS first_equity_ts
            FROM equity_snapshots
            WHERE source IN ('sim', 'real')
            GROUP BY avatar_id
        ) e ON e.avatar_id = a.id
        ORDER BY a.created_at ASC
        "#
    )
    .fetch_all(pool)
    .await
}

/// Fetches live equity snapshots since `since` (typically `now - 30 days`), ordered by avatar and timestamp.
pub async fn list_leaderboard_equity_since(
    pool: &PgPool,
    since: DateTime<Utc>,
) -> sqlx::Result<Vec<LeaderboardEquityPoint>> {
    sqlx::query_as!(
        LeaderboardEquityPoint,
        r#"
        SELECT avatar_id, ts, equity_usd
        FROM equity_snapshots
        WHERE source IN ('sim', 'real')
          AND ts >= $1
        ORDER BY avatar_id, ts ASC
        "#,
        since
    )
    .fetch_all(pool)
    .await
}
