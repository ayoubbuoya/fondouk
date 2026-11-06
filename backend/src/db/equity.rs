//! Queries for the `equity_snapshots` table (an avatar's track record).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// One equity value of an avatar at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquityPoint {
    pub ts: DateTime<Utc>,
    pub equity_usd: Decimal,
}

/// Replaces the avatar's whole backtest track record with `points` (D24).
///
/// The delete and the inserts run in **one transaction**: a crash or error
/// halfway rolls everything back, so readers see either the old track record
/// or the new one, never half of each. Rows with another `source` (`sim`,
/// `real`) are not touched.
///
/// # Errors
/// Foreign key violation for an unknown avatar; CHECK violation for a
/// negative equity; primary key violation if `points` has a duplicate `ts`.
pub async fn replace_backtest_equity(
    pool: &PgPool,
    avatar_id: Uuid,
    points: &[EquityPoint],
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;

    sqlx::query!(
        "DELETE FROM equity_snapshots WHERE avatar_id = $1 AND source = 'backtest'",
        avatar_id,
    )
    .execute(&mut *tx)
    .await?;

    // About 30 rows: one simple insert each is fast enough and easy to read.
    for point in points {
        sqlx::query!(
            r#"
            INSERT INTO equity_snapshots (avatar_id, source, ts, equity_usd)
            VALUES ($1, 'backtest', $2, $3)
            "#,
            avatar_id,
            point.ts,
            point.equity_usd,
        )
        .execute(&mut *tx)
        .await?;
    }

    // If any step above returned early with `?`, `tx` is dropped without a
    // commit and sqlx rolls it back.
    tx.commit().await
}

/// Stores one equity point for `source` (`sim` or `real`; the backtest uses
/// `replace_backtest_equity`).
///
/// Takes any executor, so the live tick can run it inside its transaction.
///
/// # Errors
/// Primary key violation when the avatar already has a point at `point.ts`
/// for this source; CHECK violation for a negative equity or unknown source.
pub async fn insert_equity_point<'e>(
    executor: impl PgExecutor<'e>,
    avatar_id: Uuid,
    source: &str,
    point: &EquityPoint,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO equity_snapshots (avatar_id, source, ts, equity_usd) VALUES ($1, $2, $3, $4)",
        avatar_id,
        source,
        point.ts,
        point.equity_usd,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Time of the avatar's newest equity point for `source`, or `None` when it
/// has none yet. The live tick accrues yield for the time since then (D33).
pub async fn last_equity_ts(
    pool: &PgPool,
    avatar_id: Uuid,
    source: &str,
) -> sqlx::Result<Option<DateTime<Utc>>> {
    sqlx::query_scalar!(
        "SELECT max(ts) FROM equity_snapshots WHERE avatar_id = $1 AND source = $2",
        avatar_id,
        source,
    )
    .fetch_one(pool)
    .await
}

/// Returns the avatar's equity points for one `source` (`backtest`, `sim` or
/// `real`), oldest first. Sources are never mixed in one result.
pub async fn list_equity(
    pool: &PgPool,
    avatar_id: Uuid,
    source: &str,
) -> sqlx::Result<Vec<EquityPoint>> {
    // Served by the (avatar_id, source, ts) primary key index.
    sqlx::query_as!(
        EquityPoint,
        r#"
        SELECT ts, equity_usd
        FROM equity_snapshots
        WHERE avatar_id = $1 AND source = $2
        ORDER BY ts
        "#,
        avatar_id,
        source,
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::avatars::{self, NewAvatar, NewConfig};
    use chrono::TimeZone;

    async fn create_avatar(pool: &PgPool) -> Uuid {
        let owner = "alice.testnet";
        avatars::ensure_user(pool, owner).await.unwrap();
        let config_id = avatars::insert_config(
            pool,
            &NewConfig {
                persona: "Prefer lending.".to_string(),
                risk: "low".to_string(),
                goal: "steady_income".to_string(),
                created_by: owner.to_string(),
            },
        )
        .await
        .unwrap();
        avatars::insert_avatar(
            pool,
            &NewAvatar {
                owner: owner.to_string(),
                config_id,
                name: "Hanna".to_string(),
                sprite: "merchant_1".to_string(),
                mode: "sim".to_string(),
                starting_capital_usd: Decimal::from(10_000),
            },
        )
        .await
        .unwrap()
    }

    fn point(day: u32, equity: i64) -> EquityPoint {
        EquityPoint {
            ts: Utc.with_ymd_and_hms(2026, 10, day, 0, 0, 0).unwrap(),
            equity_usd: Decimal::from(equity),
        }
    }

    async fn insert_sim_point(pool: &PgPool, avatar_id: Uuid, p: &EquityPoint) {
        sqlx::query!(
            "INSERT INTO equity_snapshots (avatar_id, source, ts, equity_usd) VALUES ($1, 'sim', $2, $3)",
            avatar_id,
            p.ts,
            p.equity_usd,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test]
    async fn stored_backtest_points_are_listed_oldest_first(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        let points = [point(3, 10_020), point(1, 10_000), point(2, 10_010)];

        replace_backtest_equity(&pool, avatar, &points)
            .await
            .unwrap();

        let listed = list_equity(&pool, avatar, "backtest").await.unwrap();
        assert_eq!(
            listed,
            vec![point(1, 10_000), point(2, 10_010), point(3, 10_020)]
        );
    }

    #[sqlx::test]
    async fn running_twice_keeps_only_the_second_set(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        replace_backtest_equity(&pool, avatar, &[point(1, 1), point(2, 2)])
            .await
            .unwrap();

        replace_backtest_equity(&pool, avatar, &[point(5, 50)])
            .await
            .unwrap();

        let listed = list_equity(&pool, avatar, "backtest").await.unwrap();
        assert_eq!(listed, vec![point(5, 50)]);
    }

    #[sqlx::test]
    async fn replacing_backtest_rows_leaves_sim_rows_alone(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        insert_sim_point(&pool, avatar, &point(1, 9_999)).await;

        replace_backtest_equity(&pool, avatar, &[point(1, 10_000)])
            .await
            .unwrap();

        // Same ts, different source: both rows exist, and each list sees only its own.
        assert_eq!(
            list_equity(&pool, avatar, "sim").await.unwrap(),
            vec![point(1, 9_999)]
        );
        assert_eq!(
            list_equity(&pool, avatar, "backtest").await.unwrap(),
            vec![point(1, 10_000)]
        );
    }

    #[sqlx::test]
    async fn a_failing_insert_rolls_back_the_delete(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        replace_backtest_equity(&pool, avatar, &[point(1, 10_000)])
            .await
            .unwrap();

        // Duplicate ts -> primary key violation on the second insert.
        let result = replace_backtest_equity(&pool, avatar, &[point(2, 1), point(2, 2)]).await;

        assert!(result.is_err());
        // The old track record is still there: the transaction undid the delete.
        assert_eq!(
            list_equity(&pool, avatar, "backtest").await.unwrap(),
            vec![point(1, 10_000)]
        );
    }

    #[sqlx::test]
    async fn negative_equity_is_rejected(pool: PgPool) {
        let avatar = create_avatar(&pool).await;

        let err = replace_backtest_equity(&pool, avatar, &[point(1, -1)])
            .await
            .unwrap_err();

        assert!(err.as_database_error().unwrap().is_check_violation());
    }

    #[sqlx::test]
    async fn last_equity_ts_is_the_newest_point_of_that_source(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        assert_eq!(last_equity_ts(&pool, avatar, "sim").await.unwrap(), None);

        insert_equity_point(&pool, avatar, "sim", &point(2, 10_010))
            .await
            .unwrap();
        insert_equity_point(&pool, avatar, "sim", &point(1, 10_000))
            .await
            .unwrap();
        replace_backtest_equity(&pool, avatar, &[point(9, 1)])
            .await
            .unwrap();

        assert_eq!(
            last_equity_ts(&pool, avatar, "sim").await.unwrap(),
            Some(point(2, 0).ts)
        );
    }

    #[sqlx::test]
    async fn a_second_point_at_the_same_time_is_rejected(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        insert_equity_point(&pool, avatar, "sim", &point(1, 10_000))
            .await
            .unwrap();

        let err = insert_equity_point(&pool, avatar, "sim", &point(1, 10_001))
            .await
            .unwrap_err();

        assert!(err.as_database_error().unwrap().is_unique_violation());
    }

    #[sqlx::test]
    async fn unknown_source_label_is_rejected(pool: PgPool) {
        let avatar = create_avatar(&pool).await;

        let err = sqlx::query!(
            "INSERT INTO equity_snapshots (avatar_id, source, ts, equity_usd) VALUES ($1, 'live', now(), 1)",
            avatar,
        )
        .execute(&pool)
        .await
        .unwrap_err();

        assert!(err.as_database_error().unwrap().is_check_violation());
    }
}
