//! HTTP routes. Only `/health` for now; REST and WebSocket routes come later.

use axum::{Router, routing::get};

/// Builds the router with every HTTP route of the backend.
pub fn router() -> Router {
    Router::new().route("/health", get(health))
}

/// Liveness check: answers `200 ok` while the process is up.
/// It does not check the database; that is a separate concern for later.
async fn health() -> &'static str {
    "ok"
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_returns_200_ok() {
        // `oneshot` sends one request straight into the router, no network port needed.
        let response = router()
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"ok");
    }
}
