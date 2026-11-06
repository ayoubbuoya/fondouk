//! Queries for the `yield_snapshots` table (one yield reading per village per source timestamp).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;

/// One yield reading for a village. APY values are percent per year (14.01 = 14.01%).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YieldSnapshot {
    pub village_id: String,
    /// Timestamp from the data source, not the time we fetched it.
    pub ts: DateTime<Utc>,
    pub apy_base: Decimal,
    pub apy_reward: Decimal,
    pub tvl_usd: Decimal,
}

/// Stores a snapshot unless one already exists for the same village and timestamp.
///
/// Returns `true` when a new row was written and `false` when the point was
/// already stored. Re-fetching an unchanged source point is normal (the source
/// refreshes about hourly), so a duplicate is not an error.
///
/// # Errors
/// Fails if `village_id` does not exist (foreign key) or a CHECK is violated.
pub async fn insert_snapshot(pool: &PgPool, snapshot: &YieldSnapshot) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        INSERT INTO yield_snapshots (village_id, ts, apy_base, apy_reward, tvl_usd)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (village_id, ts) DO NOTHING
        "#,
        snapshot.village_id,
        snapshot.ts,
        snapshot.apy_base,
        snapshot.apy_reward,
        snapshot.tvl_usd,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Returns the newest snapshot of each active village. Villages without any
/// snapshot yet are simply absent from the result.
pub async fn latest_snapshot_per_village(pool: &PgPool) -> sqlx::Result<Vec<YieldSnapshot>> {
    // DISTINCT ON keeps the first row per village_id; ORDER BY ts DESC makes it the newest.
    // The (village_id, ts) primary key index serves this query.
    sqlx::query_as!(
        YieldSnapshot,
        r#"
        SELECT DISTINCT ON (s.village_id)
               s.village_id, s.ts, s.apy_base, s.apy_reward, s.tvl_usd
        FROM yield_snapshots s
        JOIN villages v ON v.id = s.village_id
        WHERE v.active
        ORDER BY s.village_id, s.ts DESC
        "#
    )
    .fetch_all(pool)
    .await
}

/// Returns one snapshot per active village per **UTC day**: the last point of
/// that day. Only points with `from <= ts < to` are included. Ordered by
/// village, then day. Days without any point are simply absent (the backtest
/// handles gaps, D23).
///
/// This is the daily "newspaper" the backtest reads: DefiLlama gives about one
/// point per day plus an intraday latest point, and the indexer may also have
/// stored a few intraday points, so raw rows are not one-per-day.
pub async fn daily_history(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> sqlx::Result<Vec<YieldSnapshot>> {
    // `date_trunc('day', ts, 'UTC')` cuts days at UTC midnight. The 2-argument
    // form would use the session time zone (UTC+1 on the dev DB), which would
    // shift every day boundary by an hour.
    sqlx::query_as!(
        YieldSnapshot,
        r#"
        SELECT DISTINCT ON (s.village_id, date_trunc('day', s.ts, 'UTC'))
               s.village_id, s.ts, s.apy_base, s.apy_reward, s.tvl_usd
        FROM yield_snapshots s
        JOIN villages v ON v.id = s.village_id
        WHERE v.active AND s.ts >= $1 AND s.ts < $2
        ORDER BY s.village_id, date_trunc('day', s.ts, 'UTC'), s.ts DESC
        "#,
        from,
        to,
    )
    .fetch_all(pool)
    .await
}

/// Every stored point of one village with `ts >= since`, oldest first. Used
/// for the village sparkline (`GET /villages/:id/history`, F2.3). Served by
/// the (village_id, ts) primary key index.
pub async fn history_since(
    pool: &PgPool,
    village_id: &str,
    since: DateTime<Utc>,
) -> sqlx::Result<Vec<YieldSnapshot>> {
    sqlx::query_as!(
        YieldSnapshot,
        r#"
        SELECT village_id, ts, apy_base, apy_reward, tvl_usd
        FROM yield_snapshots
        WHERE village_id = $1 AND ts >= $2
        ORDER BY ts
        "#,
        village_id,
        since,
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Exact decimal from a string literal (avoids float rounding in test data).
    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    fn snapshot(village_id: &str, hour: u32, apy_base: Decimal) -> YieldSnapshot {
        YieldSnapshot {
            village_id: village_id.to_string(),
            ts: Utc.with_ymd_and_hms(2026, 11, 2, hour, 0, 0).unwrap(),
            apy_base,
            apy_reward: Decimal::ZERO,
            tvl_usd: d("1625385"),
        }
    }

    #[sqlx::test]
    async fn inserted_snapshot_is_returned_as_latest(pool: PgPool) {
        let s = snapshot("rhea-lend-usdc", 6, d("14.01"));

        assert!(insert_snapshot(&pool, &s).await.unwrap());

        let latest = latest_snapshot_per_village(&pool).await.unwrap();
        assert_eq!(latest, vec![s]);
    }

    #[sqlx::test]
    async fn duplicate_point_is_ignored_not_an_error(pool: PgPool) {
        let s = snapshot("rhea-lend-usdc", 6, d("14.01"));

        assert!(insert_snapshot(&pool, &s).await.unwrap());
        assert!(!insert_snapshot(&pool, &s).await.unwrap());

        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM yield_snapshots"#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[sqlx::test]
    async fn latest_picks_the_newest_point_per_village(pool: PgPool) {
        let older = snapshot("rhea-lend-usdc", 5, d("13.88"));
        let newer = snapshot("rhea-lend-usdc", 6, d("14.01"));
        let other = snapshot("rhea-lend-usdt", 6, d("13.14"));
        for s in [&newer, &older, &other] {
            insert_snapshot(&pool, s).await.unwrap();
        }

        let latest = latest_snapshot_per_village(&pool).await.unwrap();

        assert_eq!(latest, vec![newer, other]);
    }

    fn snapshot_at(village_id: &str, ts: DateTime<Utc>, apy_base: &str) -> YieldSnapshot {
        YieldSnapshot {
            village_id: village_id.to_string(),
            ts,
            apy_base: d(apy_base),
            apy_reward: Decimal::ZERO,
            tvl_usd: d("1000000"),
        }
    }

    fn utc(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
    }

    async fn insert_all(pool: &PgPool, snapshots: &[YieldSnapshot]) {
        for s in snapshots {
            insert_snapshot(pool, s).await.unwrap();
        }
    }

    #[sqlx::test]
    async fn daily_history_keeps_the_last_point_of_each_day(pool: PgPool) {
        let morning = snapshot_at("rhea-lend-usdc", utc(10, 6, 0), "13.0");
        let noon = snapshot_at("rhea-lend-usdc", utc(10, 12, 0), "13.5");
        let evening = snapshot_at("rhea-lend-usdc", utc(10, 23, 0), "14.0");
        insert_all(&pool, &[noon, evening.clone(), morning]).await;

        let history = daily_history(&pool, utc(1, 0, 0), utc(20, 0, 0))
            .await
            .unwrap();

        assert_eq!(history, vec![evening]);
    }

    #[sqlx::test]
    async fn daily_history_cuts_days_at_utc_midnight(pool: PgPool) {
        // 23:30 UTC and 00:30 UTC the next day are two different UTC days, even
        // when the session time zone (UTC+1) would put both on the same day.
        let late = snapshot_at("rhea-lend-usdc", utc(10, 23, 30), "13.0");
        let early_next = snapshot_at("rhea-lend-usdc", utc(11, 0, 30), "14.0");
        insert_all(&pool, &[late.clone(), early_next.clone()]).await;

        let history = daily_history(&pool, utc(1, 0, 0), utc(20, 0, 0))
            .await
            .unwrap();

        assert_eq!(history, vec![late, early_next]);
    }

    #[sqlx::test]
    async fn daily_history_window_includes_from_and_excludes_to(pool: PgPool) {
        let before = snapshot_at("rhea-lend-usdc", utc(9, 23, 59), "12.0");
        let at_from = snapshot_at("rhea-lend-usdc", utc(10, 0, 0), "13.0");
        let at_to = snapshot_at("rhea-lend-usdc", utc(12, 0, 0), "14.0");
        insert_all(&pool, &[before, at_from.clone(), at_to]).await;

        let history = daily_history(&pool, utc(10, 0, 0), utc(12, 0, 0))
            .await
            .unwrap();

        assert_eq!(history, vec![at_from]);
    }

    #[sqlx::test]
    async fn daily_history_skips_inactive_villages_and_orders_by_village_then_day(pool: PgPool) {
        let usdc_day2 = snapshot_at("rhea-lend-usdc", utc(11, 12, 0), "13.1");
        let usdc_day1 = snapshot_at("rhea-lend-usdc", utc(10, 12, 0), "13.0");
        let usdt_day1 = snapshot_at("rhea-lend-usdt", utc(10, 12, 0), "12.0");
        let inactive = snapshot_at("rhea-dex-frax-usdc", utc(10, 12, 0), "9.0");
        insert_all(
            &pool,
            &[
                usdt_day1.clone(),
                usdc_day2.clone(),
                inactive,
                usdc_day1.clone(),
            ],
        )
        .await;
        sqlx::query!("UPDATE villages SET active = false WHERE id = 'rhea-dex-frax-usdc'")
            .execute(&pool)
            .await
            .unwrap();

        let history = daily_history(&pool, utc(1, 0, 0), utc(20, 0, 0))
            .await
            .unwrap();

        assert_eq!(history, vec![usdc_day1, usdc_day2, usdt_day1]);
    }

    #[sqlx::test]
    async fn snapshot_for_unknown_village_is_rejected(pool: PgPool) {
        let s = snapshot("no-such-village", 6, d("5"));

        let err = insert_snapshot(&pool, &s).await.unwrap_err();

        let db_err = err.as_database_error().expect("expected a database error");
        assert!(db_err.is_foreign_key_violation());
    }

    #[sqlx::test]
    async fn history_since_returns_one_village_from_since_oldest_first(pool: PgPool) {
        insert_all(
            &pool,
            &[
                snapshot("rhea-lend-usdc", 9, d("12")),
                snapshot("rhea-lend-usdc", 3, d("10")),
                snapshot("rhea-lend-usdc", 6, d("11")),
                snapshot("rhea-lend-usdt", 7, d("13")),
            ],
        )
        .await;
        let since = Utc.with_ymd_and_hms(2026, 11, 2, 6, 0, 0).unwrap();

        let history = history_since(&pool, "rhea-lend-usdc", since).await.unwrap();

        let apys: Vec<Decimal> = history.iter().map(|s| s.apy_base).collect();
        assert_eq!(
            apys,
            vec![d("11"), d("12")],
            "from 06:00 inclusive, ordered"
        );
    }
}
