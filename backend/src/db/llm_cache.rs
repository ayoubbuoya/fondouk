//! `llm_cache`: LLM answers by input, so the same decision never pays for a
//! second call (TECH §5.2: keeps the backtest within 30 calls per character).

use serde_json::Value;
use sqlx::PgPool;

/// The cached response for `cache_key`, if any.
pub async fn get(pool: &PgPool, cache_key: &str) -> sqlx::Result<Option<Value>> {
    sqlx::query_scalar!(
        "SELECT response FROM llm_cache WHERE cache_key = $1",
        cache_key
    )
    .fetch_optional(pool)
    .await
}

/// Stores a response. A second write for the same key is ignored: the first
/// answer stays, so a decision replayed later sees the same input it had.
pub async fn put(pool: &PgPool, cache_key: &str, response: &Value) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO llm_cache (cache_key, response) VALUES ($1, $2) ON CONFLICT (cache_key) DO NOTHING",
        cache_key,
        response,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[sqlx::test]
    async fn stored_response_is_read_back_and_first_write_wins(pool: PgPool) {
        assert_eq!(get(&pool, "k").await.unwrap(), None);

        put(&pool, "k", &json!({"text": "first"})).await.unwrap();
        put(&pool, "k", &json!({"text": "second"})).await.unwrap();

        assert_eq!(
            get(&pool, "k").await.unwrap(),
            Some(json!({"text": "first"}))
        );
    }
}
