//! `GET /villages`: every active village with its latest yield and a stale flag.
//! Used by the town map (Day 6); the agents (Day 3) apply the same stale rule.

use std::collections::HashMap;
use std::time::Duration;

use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

use super::{ApiError, AppState};
use crate::db::villages::{self, Village};
use crate::db::yield_snapshots::{self, YieldSnapshot};
use crate::indexer::is_stale;

/// One village as the API returns it.
///
/// Decimals are serialized as JSON **strings** (e.g. `"14.01014"`), so the
/// browser never turns them into rounded floats. Yield fields are `null` until
/// the indexer has stored a first snapshot for the village.
#[derive(Debug, Serialize, PartialEq, Eq)]
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

/// Handler: two queries (villages, then latest snapshot per village) joined in
/// memory. With 4 villages this is simpler than one big SQL join and reuses
/// the existing db functions; it is not an N+1 pattern (always 2 queries).
pub async fn list_villages(
    State(state): State<AppState>,
) -> Result<Json<Vec<VillageDto>>, ApiError> {
    let villages = villages::list_active_villages(&state.db).await?;
    let mut latest: HashMap<String, YieldSnapshot> =
        yield_snapshots::latest_snapshot_per_village(&state.db)
            .await?
            .into_iter()
            .map(|snapshot| (snapshot.village_id.clone(), snapshot))
            .collect();

    let now = Utc::now();
    let body = villages
        .into_iter()
        .map(|village| {
            let snapshot = latest.remove(&village.id);
            to_dto(village, snapshot, now, state.village_stale_after)
        })
        .collect();
    Ok(Json(body))
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
    use crate::api::{router, tests::get};
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
        let state = AppState {
            db: pool,
            village_stale_after: TWO_HOURS,
        };

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
}
