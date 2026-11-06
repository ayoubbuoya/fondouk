//! HTTP routes: REST for reading and editing, `/ws` for live events.
//!
//! | Route | Who |
//! | --- | --- |
//! | `GET /health`, `/villages`, `/villages/{id}/history` | anyone |
//! | `POST /auth/challenge`, `/auth/near`, `/auth/logout`, `GET /auth/me` | login (D37, D38) |
//! | `GET /avatars`, `/avatars/{id}`, `…/decisions`, `…/equity` | anyone (persona only to its creator, D39) |
//! | `POST /avatars` | logged in |
//! | `PATCH /avatars/{id}` | the owner (D40) |
//! | `GET /marketplace/listings` | anyone |
//! | `POST /marketplace/claim` | logged in (turns a paid purchase into a clone) |
//! | `GET /ws` | anyone (public events only, D43) |

pub mod auth;
mod avatars;
mod marketplace;
pub mod near_rpc;
pub mod nep413;
pub mod villages;
mod ws;

use std::time::Duration;

use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use sqlx::PgPool;

use crate::live_events::LiveSender;

/// Shared data every handler can use. Cloning is cheap: the pool, the HTTP
/// client and the event sender are handles to shared objects.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    /// Stale rule threshold (`VILLAGE_STALE_AFTER_SECS`), same value the agents use.
    pub village_stale_after: Duration,
    /// Live events; each `/ws` connection subscribes to it.
    pub events: LiveSender,
    /// Outgoing HTTP (NEAR RPC at login).
    pub http: reqwest::Client,
    /// `NEAR_RPC_URL`.
    pub near_rpc_url: String,
    /// `MARKETPLACE_CONTRACT_ID`: the only contract whose events create clones.
    pub marketplace_contract_id: String,
}

/// Builds the router with every HTTP route of the backend.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/villages", get(villages::list_villages))
        .route("/villages/{id}/history", get(villages::village_history))
        .route("/auth/challenge", post(auth::create_challenge))
        .route("/auth/near", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route(
            "/avatars",
            get(avatars::list_avatars).post(avatars::create_avatar),
        )
        .route(
            "/avatars/{id}",
            get(avatars::get_avatar).patch(avatars::update_avatar),
        )
        .route("/avatars/{id}/decisions", get(avatars::list_decisions))
        .route("/avatars/{id}/equity", get(avatars::list_equity))
        .route("/marketplace/listings", get(marketplace::list_listings))
        .route("/marketplace/claim", post(marketplace::claim_purchase))
        .route("/ws", get(ws::ws_handler))
        .with_state(state)
}

/// Liveness check: answers `200 ok` while the process is up.
/// It does not check the database; that is a separate concern for later.
async fn health() -> &'static str {
    "ok"
}

/// Error returned by handlers, as `{"error": "<message>"}`.
///
/// 4xx variants carry a message meant for the client. `Internal` is a 500:
/// the client only sees a generic message and the real cause (SQL error,
/// etc.) is logged server-side, so internal details such as table names
/// never leak to the browser.
#[derive(Debug)]
pub enum ApiError {
    /// 400: the request is invalid; the text says why.
    BadRequest(String),
    /// 401: not logged in, or the login proof is wrong.
    Unauthorized,
    /// 403: logged in, but not allowed to do this.
    Forbidden,
    /// 404: no such village or avatar.
    NotFound,
    /// 500: our fault. Any `?` on an error that is not one of the above.
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(err: E) -> Self {
        Self::Internal(err.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "not logged in".to_string()),
            Self::Forbidden => (StatusCode::FORBIDDEN, "not allowed".to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            Self::Internal(err) => {
                tracing::error!("request failed: {err:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{HeaderMap, Request, header},
    };
    use http_body_util::BodyExt;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    /// State for tests, with a real test database. The NEAR RPC URL points
    /// to nothing; login tests replace it with a fake server.
    pub(crate) fn test_state(pool: PgPool) -> AppState {
        AppState {
            db: pool,
            village_stale_after: Duration::from_secs(7200),
            events: crate::live_events::channel(),
            http: reqwest::Client::new(),
            near_rpc_url: "http://127.0.0.1:9/".to_string(),
            marketplace_contract_id: "marketplace.test".to_string(),
        }
    }

    /// State whose pool never connects (`connect_lazy` only connects on first
    /// use), for tests of routes that do not touch the database.
    fn state_without_db() -> AppState {
        test_state(
            PgPoolOptions::new()
                .connect_lazy("postgres://unused@localhost/unused")
                .unwrap(),
        )
    }

    /// Sends one request straight into the router (no network port) and
    /// returns the status, the headers and the body bytes.
    pub(crate) async fn send(
        router: Router,
        request: Request<Body>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let response = router.oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, headers, body.to_vec())
    }

    /// GET without a cookie; returns the status and the body bytes.
    pub(crate) async fn get(router: Router, uri: &str) -> (StatusCode, Vec<u8>) {
        let (status, _, body) = send(router, Request::get(uri).body(Body::empty()).unwrap()).await;
        (status, body)
    }

    /// A request with an optional `Cookie` header and an optional JSON body.
    pub(crate) fn request(
        method: &str,
        uri: &str,
        cookie: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        match body {
            Some(json) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        }
    }

    /// Parses a response body as JSON.
    pub(crate) fn json(body: &[u8]) -> serde_json::Value {
        serde_json::from_slice(body).unwrap()
    }

    #[tokio::test]
    async fn health_returns_200_ok() {
        let (status, body) = get(router(state_without_db()), "/health").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"ok");
    }

    #[tokio::test]
    async fn api_error_hides_the_cause_from_the_client() {
        let response =
            ApiError::from(anyhow::anyhow!("relation \"villages\" does not exist")).into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(!text.contains("villages"));
        assert!(text.contains("internal server error"));
    }

    #[tokio::test]
    async fn client_errors_have_their_status_and_message() {
        for (error, status) in [
            (
                ApiError::BadRequest("name is too long".to_string()),
                StatusCode::BAD_REQUEST,
            ),
            (ApiError::Unauthorized, StatusCode::UNAUTHORIZED),
            (ApiError::Forbidden, StatusCode::FORBIDDEN),
            (ApiError::NotFound, StatusCode::NOT_FOUND),
        ] {
            let response = error.into_response();
            assert_eq!(response.status(), status);
        }
        let response = ApiError::BadRequest("name is too long".to_string()).into_response();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(json(&body)["error"], "name is too long");
    }
}
