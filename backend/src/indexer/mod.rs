//! Indexer: on a fixed interval, fetches the latest yield of every active village
//! from DefiLlama (D13), stores new points and records the successful fetch.
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

/// Fetches, stores and marks one village. Any error leaves `last_fetched_at` unchanged.
async fn index_village(pool: &PgPool, http: &reqwest::Client, village: &Village) -> Result<()> {
    let point = defillama::fetch_latest_point(http, &village.pool_ref).await?;

    let snapshot = YieldSnapshot {
        village_id: village.id.clone(),
        ts: point.ts,
        apy_base: point.apy_base,
        apy_reward: point.apy_reward,
        tvl_usd: point.tvl_usd,
    };
    let is_new_point = yield_snapshots::insert_snapshot(pool, &snapshot).await?;
    villages::mark_fetched(pool, &village.id, Utc::now()).await?;

    tracing::info!(
        village = %village.id,
        apy_base = %snapshot.apy_base,
        source_ts = %snapshot.ts,
        is_new_point,
        "indexed"
    );
    Ok(())
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
}
