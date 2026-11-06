//! Queries for the `villages` table.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// One yield source on the map, as stored in `villages`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Village {
    pub id: String,
    pub name: String,
    /// `lending` or `stable_lp` (enforced by a CHECK constraint in the database).
    pub kind: String,
    pub protocol: String,
    pub asset: String,
    /// DefiLlama pool uuid used by the indexer.
    pub pool_ref: String,
    pub map_x: i32,
    pub map_y: i32,
    pub active: bool,
    /// Last successful indexer fetch; `None` until the first fetch.
    pub last_fetched_at: Option<DateTime<Utc>>,
    /// Pool fee in basis points (0 for lending), used by the cost model.
    pub fee_bps: i32,
    /// Tokens the village accepts, e.g. `["USDC"]`, used by the cost model.
    pub tokens: Vec<String>,
}

/// Returns all active villages, ordered by id so callers get a stable order.
pub async fn list_active_villages(pool: &PgPool) -> sqlx::Result<Vec<Village>> {
    sqlx::query_as!(
        Village,
        r#"
        SELECT id, name, kind, protocol, asset, pool_ref, map_x, map_y, active, last_fetched_at,
               fee_bps, tokens
        FROM villages
        WHERE active
        ORDER BY id
        "#
    )
    .fetch_all(pool)
    .await
}

/// Records a successful indexer fetch for `village_id` (drives the stale rule).
///
/// `GREATEST` keeps the newest value, so an older timestamp can never move it
/// backwards (e.g. a slow request finishing after a newer one). Postgres'
/// `GREATEST` ignores NULL, so the very first fetch also works.
///
/// # Errors
/// `sqlx::Error::RowNotFound` when no village has this id.
pub async fn mark_fetched(
    pool: &PgPool,
    village_id: &str,
    fetched_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    let result = sqlx::query!(
        "UPDATE villages SET last_fetched_at = GREATEST(last_fetched_at, $2) WHERE id = $1",
        village_id,
        fetched_at,
    )
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    async fn last_fetched_at(pool: &PgPool, id: &str) -> Option<DateTime<Utc>> {
        sqlx::query_scalar!("SELECT last_fetched_at FROM villages WHERE id = $1", id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test]
    async fn mark_fetched_sets_the_timestamp(pool: PgPool) {
        let t = Utc.with_ymd_and_hms(2026, 11, 2, 7, 0, 0).unwrap();

        mark_fetched(&pool, "rhea-lend-usdc", t).await.unwrap();

        assert_eq!(last_fetched_at(&pool, "rhea-lend-usdc").await, Some(t));
    }

    #[sqlx::test]
    async fn mark_fetched_never_moves_backwards(pool: PgPool) {
        let newer = Utc.with_ymd_and_hms(2026, 11, 2, 8, 0, 0).unwrap();
        let older = Utc.with_ymd_and_hms(2026, 11, 2, 7, 0, 0).unwrap();

        mark_fetched(&pool, "rhea-lend-usdc", newer).await.unwrap();
        mark_fetched(&pool, "rhea-lend-usdc", older).await.unwrap();

        assert_eq!(last_fetched_at(&pool, "rhea-lend-usdc").await, Some(newer));
    }

    #[sqlx::test]
    async fn mark_fetched_unknown_village_is_an_error(pool: PgPool) {
        let err = mark_fetched(&pool, "no-such-village", Utc::now())
            .await
            .unwrap_err();
        assert!(matches!(err, sqlx::Error::RowNotFound));
    }

    // #[sqlx::test] gives each test a fresh temporary database with all
    // migrations applied (including the seed), then drops it afterwards.
    #[sqlx::test]
    async fn seed_creates_the_four_launch_villages(pool: PgPool) {
        let villages = list_active_villages(&pool).await.unwrap();

        let ids: Vec<&str> = villages.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "rhea-dex-frax-usdc",
                "rhea-dex-stable4",
                "rhea-lend-usdc",
                "rhea-lend-usdt"
            ]
        );
        assert!(villages.iter().all(|v| v.last_fetched_at.is_none()));
    }

    #[sqlx::test]
    async fn seed_has_the_cost_data_checked_on_chain(pool: PgPool) {
        let villages = list_active_villages(&pool).await.unwrap();
        let by_id = |id: &str| villages.iter().find(|v| v.id == id).unwrap();

        assert_eq!(by_id("rhea-lend-usdc").fee_bps, 0);
        assert_eq!(by_id("rhea-lend-usdc").tokens, ["USDC"]);
        assert_eq!(by_id("rhea-lend-usdt").tokens, ["USDT"]);
        assert_eq!(by_id("rhea-dex-stable4").fee_bps, 2);
        assert_eq!(
            by_id("rhea-dex-stable4").tokens,
            ["USDT", "USDC", "USDT.e", "USDC.e"]
        );
        assert_eq!(by_id("rhea-dex-frax-usdc").fee_bps, 2);
        assert_eq!(by_id("rhea-dex-frax-usdc").tokens, ["FRAX", "USDC"]);
    }

    #[sqlx::test]
    async fn a_village_without_tokens_is_rejected(pool: PgPool) {
        let err = sqlx::query!("UPDATE villages SET tokens = '{}' WHERE id = 'rhea-lend-usdc'")
            .execute(&pool)
            .await
            .unwrap_err();

        assert!(err.as_database_error().unwrap().is_check_violation());
    }

    #[sqlx::test]
    async fn inactive_villages_are_not_listed(pool: PgPool) {
        sqlx::query!("UPDATE villages SET active = false WHERE id = 'rhea-dex-frax-usdc'")
            .execute(&pool)
            .await
            .unwrap();

        let villages = list_active_villages(&pool).await.unwrap();

        assert_eq!(villages.len(), 3);
        assert!(villages.iter().all(|v| v.id != "rhea-dex-frax-usdc"));
    }
}
