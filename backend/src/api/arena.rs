//! Arena feed endpoint (SPEC F9, TECH §5.7).
//!
//! `GET /arena/feed`: Returns the recent messages posted to the public arena,
//! joined with avatar name and sprite, ordered newest first.

use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ApiError, AppState};
use crate::db::arena;

/// One message in the arena feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaMessageDto {
    pub id: i64,
    pub avatar_id: Uuid,
    pub avatar_name: String,
    pub sprite: String,
    pub text: String,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct FeedQuery {
    pub limit: Option<i64>,
}

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

/// `GET /arena/feed`: The newest arena messages.
pub async fn get_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Json<Vec<ArenaMessageDto>>, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let rows = arena::list_recent_messages(&state.db, limit).await?;

    let dtos = rows
        .into_iter()
        .map(|row| ArenaMessageDto {
            id: row.id,
            avatar_id: row.avatar_id,
            avatar_name: row.avatar_name,
            sprite: row.sprite,
            text: row.text,
            ts: row.ts,
        })
        .collect();

    Ok(Json(dtos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use crate::api::router;
    use crate::api::tests::{get, test_state};
    use crate::db::avatars::{self, NewAvatar, NewConfig};
    use axum::http::StatusCode;

    #[sqlx::test]
    async fn feed_returns_messages_ordered_newest_first(pool: sqlx::PgPool) {
        let owner = "alice.testnet";
        avatars::ensure_user(&pool, owner).await.unwrap();

        let cfg = avatars::insert_config(
            &pool,
            &NewConfig {
                persona: "Arena test persona".to_string(),
                risk: "low".to_string(),
                goal: "steady_income".to_string(),
                created_by: owner.to_string(),
            },
        )
        .await
        .unwrap();

        let avatar_id = avatars::insert_avatar(
            &pool,
            &NewAvatar {
                owner: owner.to_string(),
                config_id: cfg,
                name: "Amira".to_string(),
                sprite: "princess".to_string(),
                mode: "sim".to_string(),
                starting_capital_usd: Decimal::from(10000),
                active: true,
            },
        )
        .await
        .unwrap();

        let now = Utc::now();

        // Insert two messages
        let mut conn = pool.acquire().await.unwrap();
        let id1 = arena::insert_arena_message(
            &mut conn,
            avatar_id,
            None,
            now - chrono::Duration::minutes(10),
            "First message",
        )
        .await
        .unwrap();

        let id2 = arena::insert_arena_message(
            &mut conn,
            avatar_id,
            None,
            now,
            "Second message",
        )
        .await
        .unwrap();

        let state = test_state(pool);

        let (status, body) = get(router(state), "/arena/feed").await;
        assert_eq!(status, StatusCode::OK);

        let feed: Vec<ArenaMessageDto> = serde_json::from_slice(&body).unwrap();
        assert_eq!(feed.len(), 2);
        assert_eq!(feed[0].id, id2);
        assert_eq!(feed[0].avatar_name, "Amira");
        assert_eq!(feed[0].sprite, "princess");
        assert_eq!(feed[0].text, "Second message");
        assert_eq!(feed[1].id, id1);
        assert_eq!(feed[1].text, "First message");
    }
}
