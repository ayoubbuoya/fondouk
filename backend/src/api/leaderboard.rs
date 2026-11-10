//! Leaderboard endpoint (SPEC F10).
//!
//! Columns (F10.1): name, owner, mode badge (sim, real), 7d return, 30d return,
//! max drawdown, number of moves, age, clone count, price.
//! Default sort (F10.2): 30d return descending; avatars younger than 7 days of
//! live history go to the "new" tab (`is_new = true`).

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ApiError, AppState};
use crate::db::leaderboard::{self, LeaderboardEquityPoint};

/// One row in the leaderboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardEntryDto {
    pub id: Uuid,
    pub name: String,
    pub sprite: String,
    pub owner: String,
    pub mode: String,
    pub return_7d: Option<Decimal>,
    pub return_30d: Option<Decimal>,
    pub max_drawdown: Decimal,
    pub moves_count: i64,
    pub age_days: i64,
    pub created_at: DateTime<Utc>,
    pub clone_count: i64,
    pub listed: bool,
    pub price_yocto: Option<String>,
    pub is_new: bool,
}

#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    pub tab: Option<String>,
}

/// Metrics computed from an avatar's live equity history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedMetrics {
    pub return_7d: Option<Decimal>,
    pub return_30d: Option<Decimal>,
    pub max_drawdown: Decimal,
    pub age_days: i64,
    pub is_new: bool,
}

/// Computes returns, max peak-to-trough drawdown, age, and new status.
///
/// Pure function with no database dependency.
pub fn compute_avatar_metrics(
    starting_capital: Decimal,
    points: &[LeaderboardEquityPoint],
    created_at: DateTime<Utc>,
    first_equity_ts: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> ComputedMetrics {
    let age_duration = now - created_at;
    let age_days = age_duration.num_days().max(0);

    // Live history duration: from the first equity point if one exists, else created_at.
    let live_duration = first_equity_ts
        .map(|ts| now - ts)
        .unwrap_or(age_duration);
    let is_new = live_duration < Duration::days(7);

    if points.is_empty() {
        return ComputedMetrics {
            return_7d: None,
            return_30d: None,
            max_drawdown: Decimal::ZERO,
            age_days,
            is_new,
        };
    }

    let equity_end = points.last().unwrap().equity_usd;

    // 7-day return
    let cutoff_7d = now - Duration::days(7);
    let start_7d = points
        .iter()
        .find(|p| p.ts >= cutoff_7d)
        .map(|p| p.equity_usd)
        .unwrap_or(starting_capital);

    let return_7d = if start_7d > Decimal::ZERO {
        Some((equity_end - start_7d) / start_7d)
    } else {
        Some(Decimal::ZERO)
    };

    // 30-day return
    let cutoff_30d = now - Duration::days(30);
    let start_30d = points
        .iter()
        .find(|p| p.ts >= cutoff_30d)
        .map(|p| p.equity_usd)
        .unwrap_or(starting_capital);

    let return_30d = if start_30d > Decimal::ZERO {
        Some((equity_end - start_30d) / start_30d)
    } else {
        Some(Decimal::ZERO)
    };

    // Peak-to-trough max drawdown across the window
    let mut peak = Decimal::ZERO;
    let mut max_drawdown = Decimal::ZERO;
    for p in points {
        if p.equity_usd > peak {
            peak = p.equity_usd;
        } else if peak > Decimal::ZERO {
            let dd = (peak - p.equity_usd) / peak;
            if dd > max_drawdown {
                max_drawdown = dd;
            }
        }
    }

    ComputedMetrics {
        return_7d,
        return_30d,
        max_drawdown,
        age_days,
        is_new,
    }
}

/// `GET /leaderboard?tab=main|new`: public leaderboard ranking.
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<Vec<LeaderboardEntryDto>>, ApiError> {
    let avatars = leaderboard::list_leaderboard_avatars(&state.db).await?;

    let now = Utc::now();
    let since = now - Duration::days(30);
    let all_points = leaderboard::list_leaderboard_equity_since(&state.db, since).await?;

    // Group points by avatar_id
    let mut points_by_avatar: BTreeMap<Uuid, Vec<LeaderboardEquityPoint>> = BTreeMap::new();
    for point in all_points {
        points_by_avatar.entry(point.avatar_id).or_default().push(point);
    }

    let mut entries: Vec<LeaderboardEntryDto> = avatars
        .into_iter()
        .map(|avatar| {
            let points = points_by_avatar
                .get(&avatar.id)
                .map(|p| p.as_slice())
                .unwrap_or(&[]);

            let metrics = compute_avatar_metrics(
                avatar.starting_capital_usd,
                points,
                avatar.created_at,
                avatar.first_equity_ts,
                now,
            );

            LeaderboardEntryDto {
                id: avatar.id,
                name: avatar.name,
                sprite: avatar.sprite,
                owner: avatar.owner,
                mode: avatar.mode,
                return_7d: metrics.return_7d,
                return_30d: metrics.return_30d,
                max_drawdown: metrics.max_drawdown,
                moves_count: avatar.moves_count,
                age_days: metrics.age_days,
                created_at: avatar.created_at,
                clone_count: avatar.clone_count,
                listed: avatar.listed,
                price_yocto: avatar.price_yocto.map(|p| p.to_string()),
                is_new: metrics.is_new,
            }
        })
        .collect();

    // Default sort: 30d return descending, then 7d return descending, then created_at.
    entries.sort_by(|a, b| {
        b.return_30d
            .cmp(&a.return_30d)
            .then_with(|| b.return_7d.cmp(&a.return_7d))
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    // Filter by tab if requested
    if let Some(tab) = query.tab.as_deref() {
        match tab {
            "main" => entries.retain(|e| !e.is_new),
            "new" => entries.retain(|e| e.is_new),
            _ => {} // all
        }
    }

    Ok(Json(entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(n: i64) -> Decimal {
        Decimal::from(n)
    }

    #[test]
    fn metrics_acceptance_three_sample_characters() {
        let now = Utc::now();
        let day = Duration::days(1);
        let avatar_id = Uuid::from_u128(1);

        // 1. Steady gainer: 10,000 -> 10,200 -> 10,500 over 8 days
        let created_steady = now - day * 10;
        let points_steady = vec![
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 8,
                equity_usd: d(10000),
            },
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 4,
                equity_usd: d(10200),
            },
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 1,
                equity_usd: d(10500),
            },
        ];
        let steady_metrics = compute_avatar_metrics(
            d(10000),
            &points_steady,
            created_steady,
            Some(now - day * 8),
            now,
        );
        // (10500 - 10000) / 10000 = 0.05 (+5%)
        assert_eq!(steady_metrics.return_30d, Some(Decimal::new(5, 2)));
        // In the 7-day window, earliest point is at now - 4d (10200). (10500 - 10200) / 10200 = 300 / 10200
        assert!(steady_metrics.return_7d.is_some());
        assert_eq!(steady_metrics.max_drawdown, Decimal::ZERO);
        assert!(!steady_metrics.is_new, "8 days of live history = main tab");

        // 2. Volatile character: 10,000 -> 12,000 -> 9,000 -> 11,000 over 5 days
        let created_volatile = now - day * 6;
        let points_volatile = vec![
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 5,
                equity_usd: d(10000),
            },
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 3,
                equity_usd: d(12000),
            },
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 2,
                equity_usd: d(9000),
            },
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 1,
                equity_usd: d(11000),
            },
        ];
        let volatile_metrics = compute_avatar_metrics(
            d(10000),
            &points_volatile,
            created_volatile,
            Some(now - day * 5),
            now,
        );
        // (11000 - 10000) / 10000 = 0.10 (+10%)
        assert_eq!(volatile_metrics.return_30d, Some(Decimal::new(10, 2)));
        // Peak is 12000, trough is 9000 -> drop is (12000 - 9000) / 12000 = 3000 / 12000 = 0.25 (25%)
        assert_eq!(volatile_metrics.max_drawdown, Decimal::new(25, 2));
        assert!(volatile_metrics.is_new, "5 days of live history = new tab");

        // 3. Downward character: 10,000 -> 9,500 -> 9,000 over 12 days
        let created_down = now - day * 14;
        let points_down = vec![
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 12,
                equity_usd: d(10000),
            },
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 6,
                equity_usd: d(9500),
            },
            LeaderboardEquityPoint {
                avatar_id,
                ts: now - day * 1,
                equity_usd: d(9000),
            },
        ];
        let down_metrics = compute_avatar_metrics(
            d(10000),
            &points_down,
            created_down,
            Some(now - day * 12),
            now,
        );
        // (9000 - 10000) / 10000 = -0.10 (-10%)
        assert_eq!(down_metrics.return_30d, Some(Decimal::new(-10, 2)));
        // Peak is 10000, drops to 9000 -> (10000 - 9000) / 10000 = 0.10 (10%)
        assert_eq!(down_metrics.max_drawdown, Decimal::new(10, 2));
        assert!(!down_metrics.is_new);
    }

    #[test]
    fn empty_equity_history_handled_gracefully() {
        let now = Utc::now();
        let metrics = compute_avatar_metrics(d(10000), &[], now, None, now);
        assert_eq!(metrics.return_7d, None);
        assert_eq!(metrics.return_30d, None);
        assert_eq!(metrics.max_drawdown, Decimal::ZERO);
        assert!(metrics.is_new);
    }

    #[test]
    fn single_point_equity_gives_zero_return_and_zero_drawdown() {
        let now = Utc::now();
        let points = vec![LeaderboardEquityPoint {
            avatar_id: Uuid::from_u128(2),
            ts: now,
            equity_usd: d(10000),
        }];
        let metrics = compute_avatar_metrics(d(10000), &points, now, Some(now), now);
        assert_eq!(metrics.return_7d, Some(Decimal::ZERO));
        assert_eq!(metrics.return_30d, Some(Decimal::ZERO));
        assert_eq!(metrics.max_drawdown, Decimal::ZERO);
    }

    #[sqlx::test]
    async fn endpoint_returns_leaderboard_sorted_and_filtered_by_tab(pool: sqlx::PgPool) {
        use crate::api::router;
        use crate::api::tests::{get, json, test_state};
        use crate::db::avatars::{self, NewAvatar, NewConfig};
        use crate::db::equity::{self, EquityPoint};
        use axum::http::StatusCode;

        let owner = "alice.testnet";
        avatars::ensure_user(&pool, owner).await.unwrap();

        let cfg = avatars::insert_config(
            &pool,
            &NewConfig {
                persona: "Leaderboard test persona".to_string(),
                risk: "low".to_string(),
                goal: "steady_income".to_string(),
                created_by: owner.to_string(),
            },
        )
        .await
        .unwrap();

        let av1 = avatars::insert_avatar(
            &pool,
            &NewAvatar {
                owner: owner.to_string(),
                config_id: cfg,
                name: "AvatarOne".to_string(),
                sprite: "samurai".to_string(),
                mode: "sim".to_string(),
                starting_capital_usd: d(10000),
                active: true,
                listed: false,
                price_yocto: None,
            },
        )
        .await
        .unwrap();

        let now = Utc::now();
        // Give AvatarOne equity history of 10 days ago (so it is in "main" tab)
        equity::insert_equity_point(
            &pool,
            av1,
            "sim",
            &EquityPoint {
                ts: now - Duration::days(10),
                equity_usd: d(10000),
            },
        )
        .await
        .unwrap();
        equity::insert_equity_point(
            &pool,
            av1,
            "sim",
            &EquityPoint {
                ts: now - Duration::days(1),
                equity_usd: d(11000),
            },
        )
        .await
        .unwrap();

        let state = test_state(pool);

        // Fetch all
        let (status, body) = get(router(state.clone()), "/leaderboard").await;
        assert_eq!(status, StatusCode::OK);
        let list = json(&body);
        let items = list.as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "AvatarOne");
        assert_eq!(items[0]["is_new"], false);

        // Tab "main" should contain it
        let (status, body) = get(router(state.clone()), "/leaderboard?tab=main").await;
        assert_eq!(status, StatusCode::OK);
        let items = json(&body).as_array().unwrap().clone();
        assert_eq!(items.len(), 1);

        // Tab "new" should be empty
        let (status, body) = get(router(state.clone()), "/leaderboard?tab=new").await;
        assert_eq!(status, StatusCode::OK);
        let items = json(&body).as_array().unwrap().clone();
        assert_eq!(items.len(), 0);
    }
}
