//! Application settings, read once at startup from environment variables
//! (a local `.env` file is loaded into the environment first, see `main.rs`).

use std::net::SocketAddr;

use anyhow::{Context, Result};

/// Address used when `BIND_ADDR` is not set. Localhost only, so a dev
/// machine does not expose the API to the network by accident.
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";

/// All settings the backend needs to run.
///
/// `Debug` is written by hand (below) so printing the config can never leak
/// the database password.
#[derive(Clone)]
pub struct Config {
    /// Address and port the HTTP server listens on (`BIND_ADDR`).
    pub bind_addr: SocketAddr,
    /// Postgres connection string (`DATABASE_URL`). Contains the password:
    /// never log it.
    pub database_url: String,
}

impl Config {
    /// Reads settings from the process environment.
    ///
    /// # Errors
    /// Returns an error naming the variable when a value is present but invalid,
    /// so a typo in `.env` fails fast at startup instead of later at runtime.
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            bind_addr: parse_bind_addr(std::env::var("BIND_ADDR").ok())?,
            database_url: parse_database_url(std::env::var("DATABASE_URL").ok())?,
        })
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("database_url", &"<redacted>")
            .finish()
    }
}

/// Requires `DATABASE_URL` to be set and non-empty. There is no default:
/// silently connecting to a guessed database would be worse than failing.
/// The value is never echoed in the error, because it contains the password.
fn parse_database_url(value: Option<String>) -> Result<String> {
    match value {
        Some(url) if !url.trim().is_empty() => Ok(url),
        _ => anyhow::bail!("DATABASE_URL is not set (see backend/.env.example)"),
    }
}

/// Parses `BIND_ADDR`, falling back to the default when it is unset.
///
/// Kept as a pure function (value in, result out) so tests do not need to
/// change real environment variables, which is `unsafe` in Rust 2024 because
/// other threads may read the environment at the same time.
fn parse_bind_addr(value: Option<String>) -> Result<SocketAddr> {
    let raw = value.unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string());
    raw.parse()
        .with_context(|| format!("BIND_ADDR must look like 127.0.0.1:8080, got {raw:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_default_when_bind_addr_is_unset() {
        let addr = parse_bind_addr(None).unwrap();
        assert_eq!(addr.to_string(), DEFAULT_BIND_ADDR);
    }

    #[test]
    fn parses_a_valid_bind_addr() {
        let addr = parse_bind_addr(Some("0.0.0.0:3000".to_string())).unwrap();
        assert_eq!(addr.port(), 3000);
        assert!(addr.ip().is_unspecified());
    }

    #[test]
    fn rejects_an_invalid_bind_addr_with_a_clear_message() {
        let err = parse_bind_addr(Some("not-an-address".to_string())).unwrap_err();
        assert!(err.to_string().contains("BIND_ADDR"));
    }

    #[test]
    fn database_url_is_required() {
        assert!(parse_database_url(None).is_err());
        assert!(parse_database_url(Some("  ".to_string())).is_err());
    }

    #[test]
    fn debug_output_hides_the_database_password() {
        let config = Config {
            bind_addr: parse_bind_addr(None).unwrap(),
            database_url: "postgres://user:secret-pw@localhost/db".to_string(),
        };

        let printed = format!("{config:?}");

        assert!(!printed.contains("secret-pw"));
        assert!(printed.contains("<redacted>"));
    }

    #[test]
    fn accepts_a_database_url() {
        let url = "postgres://user:pw@localhost:5432/fondouk".to_string();
        assert_eq!(parse_database_url(Some(url.clone())).unwrap(), url);
    }
}
