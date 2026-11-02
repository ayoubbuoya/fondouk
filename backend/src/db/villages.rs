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
}

/// Returns all active villages, ordered by id so callers get a stable order.
pub async fn list_active_villages(pool: &PgPool) -> sqlx::Result<Vec<Village>> {
    sqlx::query_as!(
        Village,
        r#"
        SELECT id, name, kind, protocol, asset, pool_ref, map_x, map_y, active, last_fetched_at
        FROM villages
        WHERE active
        ORDER BY id
        "#
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

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
