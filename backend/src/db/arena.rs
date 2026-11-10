//! Database access for the social arena (SPEC F9, TECH §6).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// One message in the arena feed, joined with its avatar's public info.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ArenaMessageRow {
    pub id: i64,
    pub avatar_id: Uuid,
    pub avatar_name: String,
    pub sprite: String,
    pub text: String,
    pub ts: DateTime<Utc>,
}

/// Stores one message. Returns its generated id.
pub async fn insert_arena_message(
    conn: &mut PgConnection,
    avatar_id: Uuid,
    decision_id: Option<i64>,
    ts: DateTime<Utc>,
    text: &str,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        INSERT INTO arena_messages (avatar_id, decision_id, ts, text)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        avatar_id,
        decision_id,
        ts,
        text,
    )
    .fetch_one(conn)
    .await?;

    Ok(row.id)
}

/// How many messages `avatar_id` posted on or after `since`.
pub async fn avatar_message_count_since(
    pool: &PgPool,
    avatar_id: Uuid,
    since: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT COUNT(*) AS "count!"
        FROM arena_messages
        WHERE avatar_id = $1 AND ts >= $2
        "#,
        avatar_id,
        since,
    )
    .fetch_one(pool)
    .await?;

    Ok(row.count)
}

/// How many messages were posted in total across the entire arena on or after `since`.
pub async fn total_message_count_since(
    pool: &PgPool,
    since: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT COUNT(*) AS "count!"
        FROM arena_messages
        WHERE ts >= $1
        "#,
        since,
    )
    .fetch_one(pool)
    .await?;

    Ok(row.count)
}

/// The newest arena messages up to `limit`.
pub async fn list_recent_messages(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<ArenaMessageRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        ArenaMessageRow,
        r#"
        SELECT
            m.id,
            m.avatar_id,
            a.name AS avatar_name,
            a.sprite,
            m.text,
            m.ts
        FROM arena_messages m
        JOIN avatars a ON a.id = m.avatar_id
        ORDER BY m.ts DESC
        LIMIT $1
        "#,
        limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
