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

    #[sqlx::test]
    async fn snapshot_for_unknown_village_is_rejected(pool: PgPool) {
        let s = snapshot("no-such-village", 6, d("5"));

        let err = insert_snapshot(&pool, &s).await.unwrap_err();

        let db_err = err.as_database_error().expect("expected a database error");
        assert!(db_err.is_foreign_key_violation());
    }
}
