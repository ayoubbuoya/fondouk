//! Queries for the `positions` table: how much of a live (`sim`) avatar's
//! money sits in each village. Cash is in `avatars.cash_usd`.

use std::collections::{BTreeMap, HashMap};

use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// The avatar's positions, village id -> USD value. Empty when it holds only
/// cash.
pub async fn load_positions(
    pool: &PgPool,
    avatar_id: Uuid,
) -> sqlx::Result<BTreeMap<String, Decimal>> {
    let rows = sqlx::query!(
        "SELECT village_id, value_usd FROM positions WHERE avatar_id = $1",
        avatar_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.village_id, row.value_usd))
        .collect())
}

/// Every avatar's positions in one query, avatar id -> (village id -> USD).
/// Used by `GET /avatars`, so listing N avatars costs 2 queries, not N + 1.
/// Avatars holding only cash are absent.
pub async fn list_all_positions(
    pool: &PgPool,
) -> sqlx::Result<HashMap<Uuid, BTreeMap<String, Decimal>>> {
    let rows = sqlx::query!("SELECT avatar_id, village_id, value_usd FROM positions")
        .fetch_all(pool)
        .await?;
    let mut by_avatar: HashMap<Uuid, BTreeMap<String, Decimal>> = HashMap::new();
    for row in rows {
        by_avatar
            .entry(row.avatar_id)
            .or_default()
            .insert(row.village_id, row.value_usd);
    }
    Ok(by_avatar)
}

/// Replaces all of the avatar's positions with `positions`.
///
/// Takes a connection (normally `&mut *tx`) so the caller can run it in the
/// same transaction as the cash, decision and equity writes of one tick.
///
/// # Errors
/// Foreign key violation for an unknown avatar or village; CHECK violation
/// for a value <= 0 (the ledger never keeps empty positions).
pub async fn replace_positions(
    conn: &mut PgConnection,
    avatar_id: Uuid,
    positions: &BTreeMap<String, Decimal>,
) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM positions WHERE avatar_id = $1", avatar_id)
        .execute(&mut *conn)
        .await?;
    // At most 4 villages: one simple insert each.
    for (village_id, value_usd) in positions {
        sqlx::query!(
            "INSERT INTO positions (avatar_id, village_id, value_usd) VALUES ($1, $2, $3)",
            avatar_id,
            village_id,
            value_usd,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::avatars::{self, NewAvatar, NewConfig};

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
                starting_capital_usd: Decimal::from(1000),
                active: true,
            },
        )
        .await
        .unwrap()
    }

    fn positions(pairs: &[(&str, i64)]) -> BTreeMap<String, Decimal> {
        pairs
            .iter()
            .map(|(id, value)| (id.to_string(), Decimal::from(*value)))
            .collect()
    }

    #[sqlx::test]
    async fn a_new_avatar_has_no_positions(pool: PgPool) {
        let avatar = create_avatar(&pool).await;

        assert!(load_positions(&pool, avatar).await.unwrap().is_empty());
    }

    #[sqlx::test]
    async fn replace_overwrites_the_previous_positions(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        let mut conn = pool.acquire().await.unwrap();
        replace_positions(
            &mut conn,
            avatar,
            &positions(&[("rhea-lend-usdc", 600), ("rhea-lend-usdt", 400)]),
        )
        .await
        .unwrap();

        replace_positions(&mut conn, avatar, &positions(&[("rhea-dex-stable4", 1000)]))
            .await
            .unwrap();

        assert_eq!(
            load_positions(&pool, avatar).await.unwrap(),
            positions(&[("rhea-dex-stable4", 1000)])
        );
    }

    #[sqlx::test]
    async fn a_zero_position_is_rejected_by_the_database(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        let err = replace_positions(&mut conn, avatar, &positions(&[("rhea-lend-usdc", 0)]))
            .await
            .unwrap_err();

        assert!(err.as_database_error().unwrap().is_check_violation());
    }

    #[sqlx::test]
    async fn an_unknown_village_is_rejected_by_the_database(pool: PgPool) {
        let avatar = create_avatar(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        let err = replace_positions(&mut conn, avatar, &positions(&[("no-such-village", 5)]))
            .await
            .unwrap_err();

        assert!(err.as_database_error().unwrap().is_foreign_key_violation());
    }

    #[sqlx::test]
    async fn list_all_positions_groups_by_avatar(pool: PgPool) {
        let first = create_avatar(&pool).await;
        let second = create_avatar(&pool).await;
        let all_cash = create_avatar(&pool).await;
        let mut conn = pool.acquire().await.unwrap();
        let held = positions(&[("rhea-lend-usdc", 600), ("rhea-lend-usdt", 400)]);
        replace_positions(&mut conn, first, &held).await.unwrap();
        replace_positions(&mut conn, second, &positions(&[("rhea-lend-usdc", 5)]))
            .await
            .unwrap();

        let all = list_all_positions(&pool).await.unwrap();

        assert_eq!(all[&first], held);
        assert_eq!(all[&second], positions(&[("rhea-lend-usdc", 5)]));
        assert!(!all.contains_key(&all_cash));
    }
}
