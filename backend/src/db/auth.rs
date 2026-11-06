//! Queries for wallet login: `auth_challenges` (NEP-413 nonces) and
//! `sessions` (server-side logins). See HANDOFF D37, D38.
//!
//! Every function takes `now` instead of calling SQL `now()`, so tests can
//! check expiry without waiting.

use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};

/// Stores a fresh challenge nonce.
///
/// # Errors
/// Unique violation if the nonce already exists (32 random bytes: never in
/// practice); CHECK violation if it is not 32 bytes.
pub async fn insert_challenge(
    pool: &PgPool,
    nonce: &[u8],
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO auth_challenges (nonce, expires_at) VALUES ($1, $2)",
        nonce,
        expires_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Deletes challenges that expired unused. Called when a new challenge is
/// created, so the table stays small without a background job.
pub async fn delete_expired_challenges(pool: &PgPool, now: DateTime<Utc>) -> sqlx::Result<u64> {
    let result = sqlx::query!("DELETE FROM auth_challenges WHERE expires_at <= $1", now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Uses up a challenge: deletes it and returns `true` if it existed and had
/// not expired.
///
/// This is the replay protection. `DELETE … RETURNING` is atomic, so when two
/// requests race with the same signed message only one of them gets the row.
/// The caller consumes the nonce **before** checking the signature, so a
/// failed attempt also burns it.
pub async fn consume_challenge(
    pool: &PgPool,
    nonce: &[u8],
    now: DateTime<Utc>,
) -> sqlx::Result<bool> {
    let row = sqlx::query_scalar!(
        "DELETE FROM auth_challenges WHERE nonce = $1 AND expires_at > $2 RETURNING nonce",
        nonce,
        now,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

/// Stores a session. Only the sha256 of the cookie token is stored.
///
/// Takes any executor so login can run it in the same transaction that
/// creates the user.
///
/// # Errors
/// Foreign key violation if the account is not a user.
pub async fn insert_session<'e>(
    executor: impl PgExecutor<'e>,
    token_hash: &[u8],
    account_id: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO sessions (token_hash, account_id, expires_at) VALUES ($1, $2, $3)",
        token_hash,
        account_id,
        expires_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// The account logged in with this token hash, or `None` when the session is
/// unknown or expired. Served by the primary key.
pub async fn session_account(
    pool: &PgPool,
    token_hash: &[u8],
    now: DateTime<Utc>,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar!(
        "SELECT account_id FROM sessions WHERE token_hash = $1 AND expires_at > $2",
        token_hash,
        now,
    )
    .fetch_optional(pool)
    .await
}

/// Deletes one session (logout). Deleting an unknown session is a no-op.
pub async fn delete_session(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM sessions WHERE token_hash = $1", token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// Deletes expired sessions. Called on each login, so old rows do not pile up.
pub async fn delete_expired_sessions(pool: &PgPool, now: DateTime<Utc>) -> sqlx::Result<u64> {
    let result = sqlx::query!("DELETE FROM sessions WHERE expires_at <= $1", now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::avatars;
    use chrono::TimeZone;

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 6, 12, minute, 0).unwrap()
    }

    #[sqlx::test]
    async fn a_challenge_can_be_consumed_only_once(pool: PgPool) {
        let nonce = [7u8; 32];
        insert_challenge(&pool, &nonce, at(5)).await.unwrap();

        assert!(consume_challenge(&pool, &nonce, at(0)).await.unwrap());
        assert!(
            !consume_challenge(&pool, &nonce, at(0)).await.unwrap(),
            "second use must fail (replay)"
        );
    }

    #[sqlx::test]
    async fn an_expired_challenge_cannot_be_consumed(pool: PgPool) {
        let nonce = [8u8; 32];
        insert_challenge(&pool, &nonce, at(5)).await.unwrap();

        assert!(!consume_challenge(&pool, &nonce, at(5)).await.unwrap());
    }

    #[sqlx::test]
    async fn expired_challenges_are_cleaned_up(pool: PgPool) {
        insert_challenge(&pool, &[1u8; 32], at(1)).await.unwrap();
        insert_challenge(&pool, &[2u8; 32], at(9)).await.unwrap();

        assert_eq!(delete_expired_challenges(&pool, at(5)).await.unwrap(), 1);
        assert!(consume_challenge(&pool, &[2u8; 32], at(5)).await.unwrap());
    }

    #[sqlx::test]
    async fn session_is_found_until_it_expires_or_is_deleted(pool: PgPool) {
        avatars::ensure_user(&pool, "alice.testnet").await.unwrap();
        let hash = [3u8; 32];
        insert_session(&pool, &hash, "alice.testnet", at(30))
            .await
            .unwrap();

        assert_eq!(
            session_account(&pool, &hash, at(0)).await.unwrap(),
            Some("alice.testnet".to_string())
        );
        assert_eq!(session_account(&pool, &hash, at(30)).await.unwrap(), None);

        delete_session(&pool, &hash).await.unwrap();
        assert_eq!(session_account(&pool, &hash, at(0)).await.unwrap(), None);
    }

    #[sqlx::test]
    async fn expired_sessions_are_cleaned_up(pool: PgPool) {
        avatars::ensure_user(&pool, "alice.testnet").await.unwrap();
        insert_session(&pool, &[4u8; 32], "alice.testnet", at(1))
            .await
            .unwrap();
        insert_session(&pool, &[5u8; 32], "alice.testnet", at(9))
            .await
            .unwrap();

        assert_eq!(delete_expired_sessions(&pool, at(5)).await.unwrap(), 1);
    }
}
