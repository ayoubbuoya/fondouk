//! Database access. Each submodule owns the queries for one group of tables
//! (see dev-docs/2026-11-02/database-design.md).

pub mod avatars;
pub mod villages;
pub mod yield_snapshots;

use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};

/// Small pool: one backend process, a few background loops and the API.
const MAX_CONNECTIONS: u32 = 5;

/// Opens the connection pool and checks the database is reachable, so a wrong
/// `DATABASE_URL` or a stopped Postgres fails at startup, not on the first request.
///
/// # Errors
/// Returns the sqlx error when no connection can be made within 5 seconds.
pub async fn connect(database_url: &str) -> sqlx::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
}
