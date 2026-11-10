//! Hall of Fame endpoint (D74): who has the most experience, and what the
//! town unlocked lately.
//!
//! Public data only: names, sprites, levels and badge names. XP is a game
//! score, not money, so `sim` and `real` characters can share one list; each
//! row still carries its `mode` badge.

use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::{ApiError, AppState};
use crate::db::{avatars, progression as progression_db};
use crate::progression::{self, Milestone};

/// How many characters the "most experienced" list shows.
const TOP_LIMIT: usize = 10;
/// How many milestones the "latest unlocks" feed shows.
const RECENT_LIMIT: i64 = 20;

/// One character in the "most experienced" list.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FameEntryDto {
    pub id: Uuid,
    pub name: String,
    pub sprite: String,
    pub owner: String,
    pub mode: String,
    pub xp: u64,
    pub level: u32,
    pub badge_count: i64,
}

/// One line of the "latest unlocks" feed.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FameMilestoneDto {
    pub avatar_id: Uuid,
    pub avatar_name: String,
    pub sprite: String,
    /// `level` or `badge`.
    pub kind: &'static str,
    /// "Level 4" or "First Steps".
    pub label: String,
    pub reached_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct HallOfFameDto {
    /// Most XP first, at most [`TOP_LIMIT`].
    pub top: Vec<FameEntryDto>,
    /// Newest first, at most [`RECENT_LIMIT`].
    pub recent: Vec<FameMilestoneDto>,
}

/// `GET /hall-of-fame`. Four queries, whatever the number of characters.
pub async fn get_hall_of_fame(
    State(state): State<AppState>,
) -> Result<Json<HallOfFameDto>, ApiError> {
    let all_avatars = avatars::list_avatars(&state.db, None).await?;
    let all_counts = progression_db::load_counts(&state.db, None).await?;
    let badge_counts = progression_db::badge_counts(&state.db).await?;
    let recent_rows = progression_db::list_recent_milestones(&state.db, RECENT_LIMIT).await?;

    // `list_avatars` rows carry the persona; only public fields are copied.
    let mut top: Vec<FameEntryDto> = all_avatars
        .into_iter()
        .map(|avatar| {
            let counts = all_counts.get(&avatar.id).copied().unwrap_or_default();
            let xp = progression::xp(&counts);
            FameEntryDto {
                id: avatar.id,
                name: avatar.name,
                sprite: avatar.sprite,
                owner: avatar.owner,
                mode: avatar.mode,
                xp,
                level: progression::level(xp),
                badge_count: badge_counts.get(&avatar.id).copied().unwrap_or(0),
            }
        })
        .collect();
    // Most XP first. The sort is stable and `list_avatars` is oldest first,
    // so on equal XP the older character stays in front.
    top.sort_by_key(|entry| std::cmp::Reverse(entry.xp));
    top.truncate(TOP_LIMIT);

    let recent = recent_rows
        .into_iter()
        .filter_map(|row| {
            // A milestone the catalog no longer knows is skipped.
            let milestone = Milestone::parse(&row.milestone)?;
            Some(FameMilestoneDto {
                avatar_id: row.avatar_id,
                avatar_name: row.avatar_name,
                sprite: row.sprite,
                kind: milestone.kind(),
                label: milestone.label(),
                reached_at: row.reached_at,
            })
        })
        .collect();

    Ok(Json(HallOfFameDto { top, recent }))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::TimeZone;
    use rust_decimal::Decimal;
    use serde_json::{Value, json as json_value};
    use sqlx::PgPool;

    use super::*;
    use crate::api::router;
    use crate::api::tests::{get, json, test_state};
    use crate::db::avatars::{NewAvatar, NewConfig};
    use crate::db::decisions::{self, NewDecision};

    const OWNER: &str = "alice.testnet";
    const PERSONA: &str = "Secret plan: always lend on Tuesdays.";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 10, 12, 0, 0).unwrap()
    }

    async fn create_avatar(pool: &PgPool, name: &str) -> Uuid {
        avatars::ensure_user(pool, OWNER).await.unwrap();
        let config_id = avatars::insert_config(
            pool,
            &NewConfig {
                persona: PERSONA.to_string(),
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

    /// `count` valid sim decisions of one action, one per minute from `first_minute`.
    async fn decide(pool: &PgPool, avatar: Uuid, action: &str, first_minute: i64, count: i64) {
        for minute in first_minute..first_minute + count {
            decisions::insert_decision(
                pool,
                &NewDecision {
                    avatar_id: avatar,
                    ts: now() + chrono::Duration::minutes(minute),
                    source: "sim".to_string(),
                    input_hash: "0".repeat(64),
                    model: "fake-model".to_string(),
                    proposed_action: None,
                    applied_action: json_value!({ "action": action }),
                    rejected_rule: None,
                    reason_public: None,
                    cost_usd: Decimal::ZERO,
                    error_detail: None,
                },
            )
            .await
            .unwrap();
        }
    }

    async fn get_json(pool: &PgPool, uri: &str) -> Value {
        let (status, body) = get(router(test_state(pool.clone())), uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        json(&body)
    }

    #[sqlx::test]
    async fn an_empty_town_has_an_empty_hall_of_fame(pool: PgPool) {
        let body = get_json(&pool, "/hall-of-fame").await;

        assert_eq!(body, json_value!({ "top": [], "recent": [] }));
    }

    /// One story checked on every public endpoint that shows progression:
    /// Hanna did 19 holds and 1 move (115 XP, level 2), Omar did nothing.
    #[sqlx::test]
    async fn progression_is_shown_on_the_public_endpoints(pool: PgPool) {
        let omar = create_avatar(&pool, "Omar").await;
        let hanna = create_avatar(&pool, "Hanna").await;
        decide(&pool, hanna, "HOLD", 0, 19).await;
        decide(&pool, hanna, "MOVE", 19, 1).await;
        let mut conn = pool.acquire().await.unwrap();
        progression_db::insert_milestones(
            &mut conn,
            hanna,
            &["badge:first_steps".to_string()],
            now(),
        )
        .await
        .unwrap();
        progression_db::insert_milestones(
            &mut conn,
            hanna,
            &["level:2".to_string()],
            now() + chrono::Duration::minutes(19),
        )
        .await
        .unwrap();

        // Hall of Fame: most XP first, newest unlock first.
        let fame = get_json(&pool, "/hall-of-fame").await;
        let top = fame["top"].as_array().unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0]["name"], "Hanna");
        assert_eq!(top[0]["xp"], 115);
        assert_eq!(top[0]["level"], 2);
        assert_eq!(top[0]["badge_count"], 1);
        assert_eq!(top[1]["name"], "Omar");
        assert_eq!(top[1]["level"], 1);
        assert_eq!(top[1]["badge_count"], 0);
        let recent = fame["recent"].as_array().unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0]["kind"], "level");
        assert_eq!(recent[0]["label"], "Level 2");
        assert_eq!(recent[0]["avatar_name"], "Hanna");
        assert_eq!(recent[1]["kind"], "badge");
        assert_eq!(recent[1]["label"], "First Steps");
        assert!(!fame.to_string().contains("Secret plan"), "no persona");

        // The map list: level for the name tag.
        let list = get_json(&pool, "/avatars").await;
        let by_name = |name: &str| {
            list.as_array()
                .unwrap()
                .iter()
                .find(|avatar| avatar["name"] == name)
                .unwrap()
                .clone()
        };
        assert_eq!(by_name("Hanna")["level"], 2);
        assert_eq!(by_name("Hanna")["xp"], 115);
        assert_eq!(by_name("Omar")["level"], 1);

        // The character sheet: the XP bar and the whole badge catalog.
        let sheet = get_json(&pool, &format!("/avatars/{hanna}")).await;
        assert_eq!(sheet["level"], 2);
        assert_eq!(sheet["xp_into_level"], 15, "level 2 starts at 100 XP");
        assert_eq!(sheet["xp_for_next_level"], 200, "level 3 starts at 300 XP");
        let badges = sheet["badges"].as_array().unwrap();
        assert_eq!(badges.len(), progression::BADGES.len());
        assert_eq!(badges[0]["id"], "first_steps");
        assert_eq!(badges[0]["name"], "First Steps");
        assert!(badges[0]["unlocked_at"].is_string());
        assert!(
            badges[1..].iter().all(|badge| badge["unlocked_at"].is_null()),
            "the other badges are locked"
        );
        let idle = get_json(&pool, &format!("/avatars/{omar}")).await;
        assert_eq!(idle["xp"], 0);
        assert_eq!(idle["xp_for_next_level"], 100);
        assert!(
            idle["badges"]
                .as_array()
                .unwrap()
                .iter()
                .all(|badge| badge["unlocked_at"].is_null())
        );

        // The leaderboard: a level column.
        let ranks = get_json(&pool, "/leaderboard").await;
        let hanna_row = ranks
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "Hanna")
            .unwrap();
        assert_eq!(hanna_row["level"], 2);
    }
}
