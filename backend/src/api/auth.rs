//! Wallet login with NEP-413 and cookie sessions (F1, HANDOFF D37, D38).
//!
//! ```text
//! POST /auth/challenge ─> { nonce, message, recipient }        (nonce lives 5 min)
//!   wallet.signMessage(message, nonce, recipient)
//! POST /auth/near { account_id, public_key, signature, nonce }
//!   1. consume the nonce (single use: replay ─> 401)
//!   2. ed25519 check of the NEP-413 hash      (wrong ─> 401)
//!   3. RPC: key is a FullAccess key of account (no ─> 401)
//!   4. user + session row ─> Set-Cookie fondouk_session=<token>
//! ```
//!
//! The cookie holds 32 random bytes (base64url). The database stores only
//! their sha256, so a database leak gives no working cookie.

use axum::{
    Json,
    extract::{FromRequestParts, OptionalFromRequestParts, State},
    http::{HeaderMap, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ApiError, AppState, near_rpc, nep413};
use crate::db::{self, avatars};

/// The text the wallet shows the user when signing in.
pub const AUTH_MESSAGE: &str = "Sign in to Fondouk";
/// NEP-413 `recipient`: who the signature is for. A signature made for
/// another app (another recipient) does not verify here.
pub const AUTH_RECIPIENT: &str = "fondouk";

const SESSION_COOKIE: &str = "fondouk_session";
/// How long a challenge can be used: enough to open the wallet and sign.
const CHALLENGE_TTL_SECS: i64 = 5 * 60;
/// Session length (D37): 7 days.
const SESSION_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// The logged-in account. As a handler argument it rejects with 401 when
/// there is no valid session; as `Option<AuthUser>` it gives `None` instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUser {
    pub account_id: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let user =
            <Self as OptionalFromRequestParts<AppState>>::from_request_parts(parts, state).await?;
        user.ok_or(ApiError::Unauthorized)
    }
}

impl OptionalFromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    /// `None` for a missing, malformed, unknown or expired session cookie.
    /// Only a database error is an error.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Self>, ApiError> {
        let Some(token) = session_token(&parts.headers) else {
            return Ok(None);
        };
        let account = db::auth::session_account(&state.db, &token_hash(&token), Utc::now()).await?;
        Ok(account.map(|account_id| AuthUser { account_id }))
    }
}

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    /// 32 random bytes, base64. The wallet signs exactly these bytes.
    nonce: String,
    message: &'static str,
    recipient: &'static str,
}

/// `POST /auth/challenge`: a fresh single-use nonce for the wallet to sign.
/// Also deletes expired challenges, so the table needs no cleanup job.
pub async fn create_challenge(
    State(state): State<AppState>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    let now = Utc::now();
    db::auth::delete_expired_challenges(&state.db, now).await?;
    let nonce = random_32_bytes()?;
    db::auth::insert_challenge(
        &state.db,
        &nonce,
        now + TimeDelta::seconds(CHALLENGE_TTL_SECS),
    )
    .await?;
    Ok(Json(ChallengeResponse {
        nonce: STANDARD.encode(nonce),
        message: AUTH_MESSAGE,
        recipient: AUTH_RECIPIENT,
    }))
}

/// Body of `POST /auth/near`: what NEAR Connect's `signMessage` returns,
/// plus the nonce from the challenge.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    account_id: String,
    /// `ed25519:<base58>`.
    public_key: String,
    /// base64 of the 64-byte signature.
    signature: String,
    /// base64, exactly as `/auth/challenge` returned it.
    nonce: String,
}

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    account_id: String,
}

/// `POST /auth/near`: checks the signed challenge and starts a session.
///
/// Malformed input is a 400 and does not use up the nonce. Anything that
/// fails the proof (used or expired nonce, bad signature, key not a
/// full-access key of the account) is a plain 401, without saying which
/// check failed.
pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let nonce = decode_nonce(&request.nonce)?;
    if !is_valid_account_id(&request.account_id) {
        return Err(ApiError::BadRequest(
            "account_id is not a valid NEAR account id".to_string(),
        ));
    }
    let public_key = nep413::parse_public_key(&request.public_key).map_err(|_| {
        ApiError::BadRequest("public_key must look like ed25519:<base58>".to_string())
    })?;
    let signature = nep413::parse_signature(&request.signature)
        .map_err(|_| ApiError::BadRequest("signature must be base64 of 64 bytes".to_string()))?;

    let now = Utc::now();
    // 1. Use up the nonce BEFORE checking anything else: one signed message
    //    can be tried once, and two racing requests cannot both pass.
    if !db::auth::consume_challenge(&state.db, &nonce, now).await? {
        return Err(ApiError::Unauthorized);
    }
    // 2. The signature must cover our message, this nonce and our recipient.
    let hash = nep413::payload_hash(AUTH_MESSAGE, &nonce, AUTH_RECIPIENT, None);
    if !nep413::verify(&public_key, &signature, &hash) {
        return Err(ApiError::Unauthorized);
    }
    // 3. The key must be a full-access key of the claimed account. This is a
    //    network call, so it runs outside any database transaction.
    let owns_account = near_rpc::is_full_access_key(
        &state.http,
        &state.near_rpc_url,
        &request.account_id,
        &request.public_key,
    )
    .await?;
    if !owns_account {
        return Err(ApiError::Unauthorized);
    }

    // 4. Start the session.
    db::auth::delete_expired_sessions(&state.db, now).await?;
    let token = random_32_bytes()?;
    let mut tx = state.db.begin().await?;
    avatars::ensure_user(&mut *tx, &request.account_id).await?;
    db::auth::insert_session(
        &mut *tx,
        &token_hash(&token),
        &request.account_id,
        now + TimeDelta::seconds(SESSION_TTL_SECS),
    )
    .await?;
    tx.commit().await?;

    tracing::info!(account = %request.account_id, "logged in");
    let cookie = session_cookie(&URL_SAFE_NO_PAD.encode(token), SESSION_TTL_SECS);
    let body = Json(AccountResponse {
        account_id: request.account_id,
    });
    Ok(([(header::SET_COOKIE, cookie)], body).into_response())
}

/// `POST /auth/logout`: deletes the session (if any) and clears the cookie.
/// Always 204, so logging out twice is not an error.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = session_token(&headers) {
        db::auth::delete_session(&state.db, &token_hash(&token)).await?;
    }
    let cleared = session_cookie("", 0);
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, cleared)]).into_response())
}

/// `GET /auth/me`: who is logged in (401 when nobody). Lets the frontend keep
/// the login after a page reload (F1 acceptance).
pub async fn me(user: AuthUser) -> Json<AccountResponse> {
    Json(AccountResponse {
        account_id: user.account_id,
    })
}

/// The `Set-Cookie` value. `HttpOnly`: page scripts cannot read it.
/// `Secure`: HTTPS only (browsers treat `http://localhost` as secure, so dev
/// works). `SameSite=Lax`: other sites cannot send it with a POST (CSRF).
/// `Max-Age=0` deletes it.
fn session_cookie(value: &str, max_age_secs: i64) -> String {
    format!(
        "{SESSION_COOKIE}={value}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={max_age_secs}"
    )
}

/// The session token from the `Cookie` header(s), or `None` when the cookie
/// is missing or is not base64url of 32 bytes.
fn session_token(headers: &HeaderMap) -> Option<Vec<u8>> {
    let value = cookie_value(headers, SESSION_COOKIE)?;
    let token = URL_SAFE_NO_PAD.decode(value).ok()?;
    (token.len() == 32).then_some(token)
}

/// Finds one cookie by name. A browser sends `Cookie: a=1; b=2` (possibly in
/// several headers); this is the small subset of cookie parsing we need.
fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}

fn token_hash(token: &[u8]) -> [u8; 32] {
    Sha256::digest(token).into()
}

/// 32 bytes from the operating system's secure random source.
fn random_32_bytes() -> Result<[u8; 32], ApiError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|err| anyhow::anyhow!("no secure randomness available: {err}"))?;
    Ok(bytes)
}

fn decode_nonce(nonce: &str) -> Result<[u8; 32], ApiError> {
    STANDARD
        .decode(nonce)
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| ApiError::BadRequest("nonce must be base64 of 32 bytes".to_string()))
}

/// A cheap shape check before we spend an RPC call: 2 to 64 characters of
/// `a-z 0-9 . _ -` (NEAR account id alphabet). The RPC check is the real
/// proof that the account exists.
fn is_valid_account_id(account_id: &str) -> bool {
    (2..=64).contains(&account_id.len())
        && account_id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::api::near_rpc::tests::{fake_rpc, full_access_answer};
    use crate::api::nep413::tests::{public_key_string, test_key, wallet_sign};
    use crate::api::router;
    use crate::api::tests::{json, request, send, test_state};
    use axum::http::HeaderValue;
    use ed25519_dalek::SigningKey;
    use serde_json::{Value, json};
    use sqlx::PgPool;

    /// Test state whose fake NEAR RPC answers `rpc_answer` to every query.
    pub(crate) async fn state_with_rpc(pool: PgPool, rpc_answer: Value) -> AppState {
        AppState {
            near_rpc_url: fake_rpc(rpc_answer).await,
            ..test_state(pool)
        }
    }

    /// Asks for a challenge and signs it like a wallet. Returns the login body.
    async fn signed_login_body(state: &AppState, key: &SigningKey, account: &str) -> Value {
        let (_, _, body) = send(
            router(state.clone()),
            request("POST", "/auth/challenge", None, None),
        )
        .await;
        let challenge = json(&body);
        let nonce: [u8; 32] = STANDARD
            .decode(challenge["nonce"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(challenge["message"], AUTH_MESSAGE);
        assert_eq!(challenge["recipient"], AUTH_RECIPIENT);
        json!({
            "account_id": account,
            "public_key": public_key_string(key),
            "signature": wallet_sign(key, AUTH_MESSAGE, &nonce, AUTH_RECIPIENT),
            "nonce": challenge["nonce"],
        })
    }

    /// Logs `account` in through the real endpoints and returns the `Cookie`
    /// header value (`fondouk_session=…`). `state` needs a FullAccess RPC.
    pub(crate) async fn login_cookie(state: &AppState, account: &str) -> String {
        let body = signed_login_body(state, &test_key(), account).await;
        let (status, headers, _) = send(
            router(state.clone()),
            request("POST", "/auth/near", None, Some(body)),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let set_cookie = headers[header::SET_COOKIE].to_str().unwrap();
        set_cookie.split(';').next().unwrap().to_string()
    }

    async fn me_status(state: &AppState, cookie: Option<&str>) -> StatusCode {
        send(
            router(state.clone()),
            request("GET", "/auth/me", cookie, None),
        )
        .await
        .0
    }

    #[sqlx::test]
    async fn full_login_gives_a_cookie_that_me_accepts(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let body = signed_login_body(&state, &test_key(), "alice.testnet").await;

        let (status, headers, response) = send(
            router(state.clone()),
            request("POST", "/auth/near", None, Some(body)),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(json(&response)["account_id"], "alice.testnet");
        let set_cookie = headers[header::SET_COOKIE].to_str().unwrap();
        for attribute in [
            "HttpOnly",
            "Secure",
            "SameSite=Lax",
            "Path=/",
            "Max-Age=604800",
        ] {
            assert!(set_cookie.contains(attribute), "{set_cookie}");
        }

        let cookie = set_cookie.split(';').next().unwrap();
        let (status, _, body) = send(
            router(state.clone()),
            request("GET", "/auth/me", Some(cookie), None),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json(&body)["account_id"], "alice.testnet");
    }

    #[sqlx::test]
    async fn the_same_signed_message_cannot_log_in_twice(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let body = signed_login_body(&state, &test_key(), "alice.testnet").await;

        let first = send(
            router(state.clone()),
            request("POST", "/auth/near", None, Some(body.clone())),
        )
        .await;
        let replay = send(
            router(state.clone()),
            request("POST", "/auth/near", None, Some(body)),
        )
        .await;

        assert_eq!(first.0, StatusCode::OK);
        assert_eq!(replay.0, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn a_signature_by_another_key_is_refused(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let mut body = signed_login_body(&state, &test_key(), "alice.testnet").await;
        // Claim a different key than the one that signed.
        body["public_key"] = json!(public_key_string(&SigningKey::from_bytes(&[9u8; 32])));

        let (status, _, _) = send(
            router(state.clone()),
            request("POST", "/auth/near", None, Some(body)),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn a_function_call_key_does_not_prove_the_account(pool: PgPool) {
        let answer = json!({ "result": { "permission": { "FunctionCall": {} } } });
        let state = state_with_rpc(pool, answer).await;
        let body = signed_login_body(&state, &test_key(), "alice.testnet").await;

        let (status, _, _) = send(
            router(state.clone()),
            request("POST", "/auth/near", None, Some(body)),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn a_nonce_we_never_issued_is_refused(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let nonce = [5u8; 32];
        let body = json!({
            "account_id": "alice.testnet",
            "public_key": public_key_string(&test_key()),
            "signature": wallet_sign(&test_key(), AUTH_MESSAGE, &nonce, AUTH_RECIPIENT),
            "nonce": STANDARD.encode(nonce),
        });

        let (status, _, _) = send(
            router(state.clone()),
            request("POST", "/auth/near", None, Some(body)),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn malformed_login_input_is_a_400_and_keeps_the_nonce(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let good = signed_login_body(&state, &test_key(), "alice.testnet").await;

        for (field, bad) in [
            ("account_id", "Alice.Testnet"),
            ("account_id", "a"),
            ("nonce", "AAAA"),
            ("public_key", "ed25519:nope"),
            ("signature", "!!"),
        ] {
            let mut body = good.clone();
            body[field] = json!(bad);
            let (status, _, _) = send(
                router(state.clone()),
                request("POST", "/auth/near", None, Some(body)),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{field} = {bad}");
        }

        // The nonce was not used up by the bad requests.
        let (status, _, _) = send(
            router(state.clone()),
            request("POST", "/auth/near", None, Some(good)),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[sqlx::test]
    async fn logout_ends_the_session(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let cookie = login_cookie(&state, "alice.testnet").await;

        let (status, headers, _) = send(
            router(state.clone()),
            request("POST", "/auth/logout", Some(&cookie), None),
        )
        .await;

        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(
            headers[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        assert_eq!(
            me_status(&state, Some(&cookie)).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[sqlx::test]
    async fn an_expired_session_is_refused(pool: PgPool) {
        let state = state_with_rpc(pool.clone(), full_access_answer()).await;
        let cookie = login_cookie(&state, "alice.testnet").await;
        sqlx::query!("UPDATE sessions SET expires_at = now() - interval '1 second'")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            me_status(&state, Some(&cookie)).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[sqlx::test]
    async fn me_without_a_valid_cookie_is_401(pool: PgPool) {
        let state = test_state(pool);

        assert_eq!(me_status(&state, None).await, StatusCode::UNAUTHORIZED);
        assert_eq!(
            me_status(&state, Some("fondouk_session=garbage")).await,
            StatusCode::UNAUTHORIZED
        );
        let unknown = format!("fondouk_session={}", URL_SAFE_NO_PAD.encode([1u8; 32]));
        assert_eq!(
            me_status(&state, Some(&unknown)).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn cookie_value_finds_the_cookie_among_others() {
        let mut headers = HeaderMap::new();
        headers.append(header::COOKIE, HeaderValue::from_static("theme=dark"));
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("a=1;  fondouk_session=abc ; b=2"),
        );

        assert_eq!(cookie_value(&headers, SESSION_COOKIE), Some("abc"));
        assert_eq!(cookie_value(&headers, "missing"), None);
        assert_eq!(cookie_value(&HeaderMap::new(), SESSION_COOKIE), None);
    }

    #[test]
    fn session_token_must_be_32_bytes_of_base64url() {
        let mut headers = HeaderMap::new();
        let short = format!("fondouk_session={}", URL_SAFE_NO_PAD.encode([1u8; 16]));
        headers.insert(header::COOKIE, HeaderValue::from_str(&short).unwrap());
        assert_eq!(session_token(&headers), None);

        let good = format!("fondouk_session={}", URL_SAFE_NO_PAD.encode([1u8; 32]));
        headers.insert(header::COOKIE, HeaderValue::from_str(&good).unwrap());
        assert_eq!(session_token(&headers), Some(vec![1u8; 32]));
    }

    #[test]
    fn account_id_shape_check() {
        for good in ["alice.testnet", "a1", "my-app_v2.near", &"a".repeat(64)] {
            assert!(is_valid_account_id(good), "{good}");
        }
        for bad in ["", "a", "Alice.near", "alice@near", "a b", &"a".repeat(65)] {
            assert!(!is_valid_account_id(bad), "{bad}");
        }
    }
}
