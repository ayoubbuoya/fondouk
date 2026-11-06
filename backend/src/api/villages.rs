//! `GET /villages`: every active village with its latest yield and a stale flag.
//! `GET /villages/{id}/history`: its yield points, for the sparkline (F2.3).
//! Used by the town map (Day 6); the agents (Day 3) apply the same stale rule.

use std::collections::HashMap;
use std::time::Duration;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, TimeDelta, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::{ApiError, AppState};
use crate::db::villages::{self, Village};
use crate::db::yield_snapshots::{self, YieldSnapshot};
use crate::indexer::is_stale;

/// History window when `?days=` is not given: a 7-day sparkline (F2.3).
const DEFAULT_HISTORY_DAYS: i64 = 7;
/// Longest window: the backtest window, and all DefiLlama gives us anyway.
const MAX_HISTORY_DAYS: i64 = 30;

/// One village as the API returns it.
///
/// Decimals are serialized as JSON **strings** (e.g. `"14.01014"`), so the
/// browser never turns them into rounded floats. Yield fields are `null` until
/// the indexer has stored a first snapshot for the village.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VillageDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub asset: String,
    pub map_x: i32,
    pub map_y: i32,
    /// Percent per year (APY), e.g. `"14.01014"`.
    pub apy_base: Option<Decimal>,
    pub apy_reward: Option<Decimal>,
    pub tvl_usd: Option<Decimal>,
    /// Timestamp of the snapshot at the data source (DefiLlama).
    pub source_ts: Option<DateTime<Utc>>,
    /// Last successful indexer fetch.
    pub last_fetched_at: Option<DateTime<Utc>>,
    /// `true` means agents must not move funds into this village (SPEC section 6).
    pub stale: bool,
}

/// One yield point of a village's history.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct YieldPointDto {
    pub ts: DateTime<Utc>,
    pub apy_base: Decimal,
    pub apy_reward: Decimal,
    pub tvl_usd: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    days: Option<i64>,
}

/// Handler for `GET /villages`.
pub async fn list_villages(
    State(state): State<AppState>,
) -> Result<Json<Vec<VillageDto>>, ApiError> {
    Ok(Json(
        current_villages(&state.db, Utc::now(), state.village_stale_after).await?,
    ))
}

/// Every active village with its latest yield and stale flag. Shared by
/// `GET /villages` and the indexer's `apr_update` event, so both always say
/// the same thing.
///
/// Two queries (villages, then latest snapshot per village) joined in memory.
/// With 4 villages this is simpler than one big SQL join and reuses the
/// existing db functions; it is not an N+1 pattern (always 2 queries).
pub async fn current_villages(
    pool: &PgPool,
    now: DateTime<Utc>,
    stale_after: Duration,
) -> sqlx::Result<Vec<VillageDto>> {
    let villages = villages::list_active_villages(pool).await?;
    let mut latest: HashMap<String, YieldSnapshot> =
        yield_snapshots::latest_snapshot_per_village(pool)
            .await?
            .into_iter()
            .map(|snapshot| (snapshot.village_id.clone(), snapshot))
            .collect();

    Ok(villages
        .into_iter()
        .map(|village| {
            let snapshot = latest.remove(&village.id);
            to_dto(village, snapshot, now, stale_after)
        })
        .collect())
}

/// `GET /villages/{id}/history?days=7`: every stored yield point of the last
/// `days` days (1 to 30, default 7), oldest first. 404 for an unknown or
/// inactive village.
pub async fn village_history(
    State(state): State<AppState>,
    Path(village_id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<YieldPointDto>>, ApiError> {
    let days = query.days.unwrap_or(DEFAULT_HISTORY_DAYS);
    if !(1..=MAX_HISTORY_DAYS).contains(&days) {
        return Err(ApiError::BadRequest(format!(
            "days must be between 1 and {MAX_HISTORY_DAYS}"
        )));
    }
    let known = villages::list_active_villages(&state.db)
        .await?
        .iter()
        .any(|village| village.id == village_id);
    if !known {
        return Err(ApiError::NotFound);
    }

    let since = Utc::now() - TimeDelta::days(days);
    let points = yield_snapshots::history_since(&state.db, &village_id, since)
        .await?
        .into_iter()
        .map(|snapshot| YieldPointDto {
            ts: snapshot.ts,
            apy_base: snapshot.apy_base,
            apy_reward: snapshot.apy_reward,
            tvl_usd: snapshot.tvl_usd,
        })
        .collect();
    Ok(Json(points))
}

/// Pure mapping from database rows to the API shape, so the stale flag and
/// the `null` handling are unit-tested without a database.
fn to_dto(
    village: Village,
    snapshot: Option<YieldSnapshot>,
    now: DateTime<Utc>,
    stale_after: Duration,
) -> VillageDto {
    VillageDto {
        stale: is_stale(village.last_fetched_at, now, stale_after),
        apy_base: snapshot.as_ref().map(|s| s.apy_base),
        apy_reward: snapshot.as_ref().map(|s| s.apy_reward),
        tvl_usd: snapshot.as_ref().map(|s| s.tvl_usd),
        source_ts: snapshot.as_ref().map(|s| s.ts),
        last_fetched_at: village.last_fetched_at,
        id: village.id,
        name: village.name,
        kind: village.kind,
        asset: village.asset,
        map_x: village.map_x,
        map_y: village.map_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::router;
    use crate::api::tests::{get, json, test_state};
    use axum::http::StatusCode;
    use chrono::TimeZone;
    use sqlx::PgPool;

    const TWO_HOURS: Duration = Duration::from_secs(7200);

    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 4, hour, 0, 0).unwrap()
    }

    fn village(last_fetched_at: Option<DateTime<Utc>>) -> Village {
        Village {
            id: "rhea-lend-usdc".to_string(),
            name: "Rhea Lend USDC".to_string(),
            kind: "lending".to_string(),
            protocol: "rhea".to_string(),
            asset: "USDC".to_string(),
            pool_ref: "1a54c607-d829-5621-87a7-e3b7debc51a1".to_string(),
            map_x: 100,
            map_y: 100,
            active: true,
            last_fetched_at,
            fee_bps: 0,
            tokens: vec!["USDC".to_string()],
        }
    }

    fn snapshot() -> YieldSnapshot {
        YieldSnapshot {
            village_id: "rhea-lend-usdc".to_string(),
            ts: at(6),
            apy_base: d("14.01014"),
            apy_reward: Decimal::ZERO,
            tvl_usd: d("1625385"),
        }
    }

    #[test]
    fn fresh_village_with_snapshot_is_not_stale() {
        let dto = to_dto(village(Some(at(9))), Some(snapshot()), at(10), TWO_HOURS);

        assert!(!dto.stale);
        assert_eq!(dto.apy_base, Some(d("14.01014")));
        assert_eq!(dto.source_ts, Some(at(6)));
    }

    #[test]
    fn village_without_snapshot_has_nulls_and_is_stale() {
        let dto = to_dto(village(None), None, at(10), TWO_HOURS);

        assert!(dto.stale);
        assert_eq!(dto.apy_base, None);
        assert_eq!(dto.tvl_usd, None);
        assert_eq!(dto.source_ts, None);
    }

    #[test]
    fn old_fetch_makes_the_village_stale_even_with_data() {
        let dto = to_dto(village(Some(at(7))), Some(snapshot()), at(10), TWO_HOURS);

        assert!(dto.stale);
        assert_eq!(
            dto.apy_base,
            Some(d("14.01014")),
            "last known value is still shown"
        );
    }

    #[test]
    fn decimals_are_serialized_as_strings() {
        let dto = to_dto(village(Some(at(9))), Some(snapshot()), at(10), TWO_HOURS);

        let json = serde_json::to_value(&dto).unwrap();

        assert_eq!(json["apy_base"], "14.01014");
        assert_eq!(json["tvl_usd"], "1625385");
        assert!(json["apy_reward"].is_string());
    }

    #[sqlx::test]
    async fn list_returns_all_active_villages_with_latest_yield(pool: PgPool) {
        yield_snapshots::insert_snapshot(&pool, &snapshot())
            .await
            .unwrap();
        villages::mark_fetched(&pool, "rhea-lend-usdc", Utc::now())
            .await
            .unwrap();
        let state = test_state(pool);

        let (status, body) = get(router(state), "/villages").await;

        assert_eq!(status, StatusCode::OK);
        let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let list = list.as_array().unwrap();
        assert_eq!(list.len(), 4);

        let usdc = list.iter().find(|v| v["id"] == "rhea-lend-usdc").unwrap();
        assert_eq!(usdc["apy_base"], "14.01014");
        assert_eq!(usdc["stale"], false);

        let never_fetched = list.iter().find(|v| v["id"] == "rhea-lend-usdt").unwrap();
        assert!(never_fetched["apy_base"].is_null());
        assert_eq!(never_fetched["stale"], true);
    }

    async fn insert_point(pool: &PgPool, village_id: &str, days_ago: i64, apy: &str) {
        yield_snapshots::insert_snapshot(
            pool,
            &YieldSnapshot {
                village_id: village_id.to_string(),
                ts: Utc::now() - TimeDelta::days(days_ago),
                apy_base: d(apy),
                apy_reward: Decimal::ZERO,
                tvl_usd: d("1000"),
            },
        )
        .await
        .unwrap();
    }

    #[sqlx::test]
    async fn history_returns_the_window_oldest_first(pool: PgPool) {
        insert_point(&pool, "rhea-lend-usdc", 20, "9").await;
        insert_point(&pool, "rhea-lend-usdc", 5, "10").await;
        insert_point(&pool, "rhea-lend-usdc", 1, "11").await;
        insert_point(&pool, "rhea-lend-usdt", 1, "99").await;
        let state = test_state(pool);

        let (status, body) = get(router(state.clone()), "/villages/rhea-lend-usdc/history").await;
        assert_eq!(status, StatusCode::OK);
        let apys: Vec<String> = json(&body)
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["apy_base"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(apys, vec!["10", "11"], "default 7 days, this village only");

        let (_, body) = get(router(state), "/villages/rhea-lend-usdc/history?days=30").await;
        assert_eq!(json(&body).as_array().unwrap().len(), 3);
    }

    #[sqlx::test]
    async fn history_rejects_bad_days_and_unknown_villages(pool: PgPool) {
        let state = test_state(pool);

        for uri in [
            "/villages/rhea-lend-usdc/history?days=0",
            "/villages/rhea-lend-usdc/history?days=31",
        ] {
            assert_eq!(
                get(router(state.clone()), uri).await.0,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            get(router(state), "/villages/nope/history").await.0,
            StatusCode::NOT_FOUND
        );
    }
}
