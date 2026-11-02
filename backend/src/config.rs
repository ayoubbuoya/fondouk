//! Application settings, read once at startup from environment variables
//! (a local `.env` file is loaded into the environment first, see `main.rs`).

use std::net::SocketAddr;

use anyhow::{Context, Result};

/// Address used when `BIND_ADDR` is not set. Localhost only, so a dev
/// machine does not expose the API to the network by accident.
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";

/// All settings the backend needs to run.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address and port the HTTP server listens on (`BIND_ADDR`).
    pub bind_addr: SocketAddr,
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
        })
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
}
