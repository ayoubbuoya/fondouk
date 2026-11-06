//! Queries for `processed_purchases`: which marketplace purchases already
//! became a clone. The transaction hash is the primary key, so it is also the
//! replay protection (see migration 0008).

use rust_decimal::Decimal;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// A purchase that was turned into a clone.
#[derive(Debug, Clone)]
pub struct NewPurchase {
    pub tx_hash: String,
    pub source_avatar_id: Uuid,
    pub clone_avatar_id: Uuid,
    pub buyer: String,
    pub creator: String,
    pub price_yocto: Decimal,
}

/// The clone created for `tx_hash`, or `None` when it was not claimed yet.
pub async fn find_clone_for_tx(pool: &PgPool, tx_hash: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar!(
        "SELECT clone_avatar_id FROM processed_purchases WHERE tx_hash = $1",
        tx_hash,
    )
    .fetch_optional(pool)
    .await
}

/// Records a purchase. Returns `false` when this `tx_hash` is already
/// recorded (someone else claimed it first), `true` when the row was inserted.
///
/// `ON CONFLICT DO NOTHING` makes two claims at the same moment safe: the
/// second insert waits for the first transaction, then inserts nothing.
pub async fn insert_purchase<'e>(
    executor: impl PgExecutor<'e>,
    purchase: &NewPurchase,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        INSERT INTO processed_purchases
            (tx_hash, source_avatar_id, clone_avatar_id, buyer, creator, price_yocto)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (tx_hash) DO NOTHING
        "#,
        purchase.tx_hash,
        purchase.source_avatar_id,
        purchase.clone_avatar_id,
        purchase.buyer,
        purchase.creator,
        purchase.price_yocto,
    )
    .execute(executor)
    .await?;
    Ok(result.rows_affected() == 1)
}
