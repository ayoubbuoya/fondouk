//! Indexer: on a fixed interval, fetches the yield chart of every active village
//! from DefiLlama (D13), stores every point not seen before (this doubles as the
//! backtest history import, D22) and records the successful fetch.
//!
//! A failing village is logged and skipped; it is NOT marked as fetched, so it
//! becomes stale on its own after `VILLAGE_STALE_AFTER_SECS` (D15, D19).

pub mod defillama;

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::PgPool;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use crate::db::villages::{self, Village};
use crate::db::yield_snapshots::{self, YieldSnapshot};
use defillama::YieldPoint;

/// Runs until `shutdown` turns `true`. The first round starts immediately.
///
/// Shutdown is only checked between rounds, so a round in progress always
/// finishes its database writes before the indexer stops.
pub async fn run(
    pool: PgPool,
    http: reqwest::Client,
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) {
    tracing::info!(interval_secs = interval.as_secs(), "indexer started");
    let mut ticker = tokio::time::interval(interval);
    // If a round runs late (slow network), wait a full interval before the next
    // one instead of firing several catch-up rounds back to back.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => index_once(&pool, &http).await,
            () = stop_requested(&mut shutdown) => break,
        }
    }
    tracing::info!("indexer stopped");
}

/// Resolves once the shutdown flag is `true`, or when the sender is dropped
/// (also treated as a shutdown).
///
/// `wait_for` returns a read guard that is not `Send`; doing the wait inside
/// this function drops the guard before returning, so `run` stays spawnable.
pub async fn stop_requested(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|stop| *stop).await;
}

/// One round: every active village, one after the other. There are only 4
/// villages, so sequential requests are simple and gentle on the free API.
pub async fn index_once(pool: &PgPool, http: &reqwest::Client) {
    let villages = match villages::list_active_villages(pool).await {
        Ok(villages) => villages,
        Err(err) => {
            tracing::error!("indexer could not load villages: {err}");
            return;
        }
    };

    for village in &villages {
        if let Err(err) = index_village(pool, http, village).await {
            // `{:#}` prints the whole context chain, e.g. "request to ... failed: timeout".
            tracing::warn!(village = %village.id, "indexer fetch failed: {err:#}");
        }
    }
}

/// Fetches the whole chart of one village, stores every point it does not have
/// yet (this is also the backtest history import, D22), then marks the village
/// as fetched. Any error leaves `last_fetched_at` unchanged.
async fn index_village(pool: &PgPool, http: &reqwest::Client, village: &Village) -> Result<()> {
    let chart = defillama::fetch_chart(http, &village.pool_ref).await?;

    let new_points = store_points(pool, &village.id, &chart.points).await?;
    villages::mark_fetched(pool, &village.id, Utc::now()).await?;

    let newest = chart.newest();
    tracing::info!(
        village = %village.id,
        apy_base = %newest.apy_base,
        source_ts = %newest.ts,
        new_points,
        skipped_points = chart.skipped,
        "indexed"
    );
    Ok(())
}

/// Stores the points of one village and returns how many were new.
///
/// Already-stored points are ignored by `insert_snapshot`, so calling this
/// every round with the full chart (about 45 points) only writes the new ones.
/// No transaction: each insert is idempotent on its own, so if a round stops
/// halfway the next round simply stores the rest.
async fn store_points(pool: &PgPool, village_id: &str, points: &[YieldPoint]) -> Result<usize> {
    let mut new_points = 0;
    for point in points {
        let snapshot = YieldSnapshot {
            village_id: village_id.to_string(),
            ts: point.ts,
            apy_base: point.apy_base,
            apy_reward: point.apy_reward,
            tvl_usd: point.tvl_usd,
        };
        if yield_snapshots::insert_snapshot(pool, &snapshot).await? {
            new_points += 1;
        }
    }
    Ok(new_points)
}

/// The stale rule (D15 + Option A of D19): a village is stale when the indexer
/// has never fetched it, or its last successful fetch is older than `stale_after`.
/// Stale villages cannot receive funds (SPEC section 6).
pub fn is_stale(
    last_fetched_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    stale_after: Duration,
) -> bool {
    let Some(last) = last_fetched_at else {
        return true;
    };
    // A huge configured value cannot overflow: it just means "never stale".
    let limit = TimeDelta::from_std(stale_after).unwrap_or(TimeDelta::MAX);
    now - last > limit
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const TWO_HOURS: Duration = Duration::from_secs(2 * 3600);

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 2, hour, minute, 0).unwrap()
    }

    #[test]
    fn never_fetched_village_is_stale() {
        assert!(is_stale(None, at(10, 0), TWO_HOURS));
    }

    #[test]
    fn recently_fetched_village_is_not_stale() {
        assert!(!is_stale(Some(at(9, 0)), at(10, 0), TWO_HOURS));
    }

    #[test]
    fn exactly_at_the_limit_is_not_stale_yet() {
        assert!(!is_stale(Some(at(8, 0)), at(10, 0), TWO_HOURS));
    }

    #[test]
    fn older_than_the_limit_is_stale() {
        assert!(is_stale(Some(at(7, 59)), at(10, 0), TWO_HOURS));
    }

    fn point(day: u32, apy_base: &str) -> YieldPoint {
        YieldPoint {
            ts: Utc.with_ymd_and_hms(2026, 10, day, 23, 0, 0).unwrap(),
            apy_base: rust_decimal::Decimal::from_str_exact(apy_base).unwrap(),
            apy_reward: rust_decimal::Decimal::ZERO,
            tvl_usd: rust_decimal::Decimal::from(1_000_000),
        }
    }

    async fn snapshot_count(pool: &PgPool) -> i64 {
        sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM yield_snapshots"#)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test]
    async fn storing_a_chart_writes_every_point(pool: PgPool) {
        let points = [point(1, "13.1"), point(2, "13.4"), point(3, "14.0")];

        let new_points = store_points(&pool, "rhea-lend-usdc", &points)
            .await
            .unwrap();

        assert_eq!(new_points, 3);
        assert_eq!(snapshot_count(&pool).await, 3);
    }

    #[sqlx::test]
    async fn storing_the_same_chart_again_writes_only_new_points(pool: PgPool) {
        let first_round = [point(1, "13.1"), point(2, "13.4")];
        store_points(&pool, "rhea-lend-usdc", &first_round)
            .await
            .unwrap();

        // Next round: the chart now has one more day at the end.
        let second_round = [point(1, "13.1"), point(2, "13.4"), point(3, "14.0")];
        let new_points = store_points(&pool, "rhea-lend-usdc", &second_round)
            .await
            .unwrap();

        assert_eq!(new_points, 1);
        assert_eq!(snapshot_count(&pool).await, 3);
    }
}
