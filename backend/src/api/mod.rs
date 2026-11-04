//! HTTP routes. REST and WebSocket routes are added here as features land.

mod villages;

use std::time::Duration;

use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use sqlx::PgPool;

/// Shared, read-only data every handler can use. Cloning is cheap: `PgPool`
/// is a handle to the same pool, and `Duration` is a small copy.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    /// Stale rule threshold (`VILLAGE_STALE_AFTER_SECS`), same value the agents use.
    pub village_stale_after: Duration,
}

/// Builds the router with every HTTP route of the backend.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/villages", get(villages::list_villages))
        .with_state(state)
}

/// Liveness check: answers `200 ok` while the process is up.
/// It does not check the database; that is a separate concern for later.
async fn health() -> &'static str {
    "ok"
}

/// Error returned by handlers. The client only sees a generic 500 message;
/// the real cause (SQL error, etc.) is logged server-side, so internal details
/// such as table names never leak to the browser.
pub struct ApiError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::error!("request failed: {:#}", self.0);
        let body = Json(json!({ "error": "internal server error" }));
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    /// State whose pool never connects (`connect_lazy` only connects on first
    /// use), for tests of routes that do not touch the database.
    fn state_without_db() -> AppState {
        AppState {
            db: PgPoolOptions::new()
                .connect_lazy("postgres://unused@localhost/unused")
                .unwrap(),
            village_stale_after: Duration::from_secs(7200),
        }
    }

    /// Sends one GET request straight into the router (no network port) and
    /// returns the status and the body bytes.
    pub(crate) async fn get(router: Router, uri: &str) -> (StatusCode, Vec<u8>) {
        let response = router
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, body.to_vec())
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
}
