//! Application settings, read once at startup from environment variables
//! (a local `.env` file is loaded into the environment first, see `main.rs`).

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result};

/// Address used when `BIND_ADDR` is not set. Localhost only, so a dev
/// machine does not expose the API to the network by accident.
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";

/// Indexer fetch interval when `INDEXER_INTERVAL_SECS` is unset: 1 hour (D19),
/// matching how often DefiLlama refreshes.
const DEFAULT_INDEXER_INTERVAL_SECS: u64 = 3600;

/// Lowest accepted interval. Protects the free DefiLlama API from a typo such
/// as `INDEXER_INTERVAL_SECS=1`.
const MIN_INDEXER_INTERVAL_SECS: u64 = 60;

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
    /// Time between two indexer rounds (`INDEXER_INTERVAL_SECS`, default 1 hour).
    pub indexer_interval: Duration,
    /// A village with no successful fetch for longer than this is stale
    /// (`VILLAGE_STALE_AFTER_SECS`, default 2 x the indexer interval).
    pub village_stale_after: Duration,
}

impl Config {
    /// Reads settings from the process environment.
    ///
    /// # Errors
    /// Returns an error naming the variable when a value is present but invalid,
    /// so a typo in `.env` fails fast at startup instead of later at runtime.
    pub fn from_env() -> Result<Self> {
        let indexer_interval = parse_indexer_interval(std::env::var("INDEXER_INTERVAL_SECS").ok())?;
        Ok(Self {
            bind_addr: parse_bind_addr(std::env::var("BIND_ADDR").ok())?,
            database_url: parse_database_url(std::env::var("DATABASE_URL").ok())?,
            indexer_interval,
            village_stale_after: parse_stale_after(
                std::env::var("VILLAGE_STALE_AFTER_SECS").ok(),
                indexer_interval,
            )?,
        })
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("database_url", &"<redacted>")
            .field("indexer_interval", &self.indexer_interval)
            .field("village_stale_after", &self.village_stale_after)
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

/// Parses `INDEXER_INTERVAL_SECS` (whole seconds). Default 1 hour, minimum 60 s.
fn parse_indexer_interval(value: Option<String>) -> Result<Duration> {
    let Some(raw) = value else {
        return Ok(Duration::from_secs(DEFAULT_INDEXER_INTERVAL_SECS));
    };
    let secs: u64 = raw
        .trim()
        .parse()
        .with_context(|| format!("INDEXER_INTERVAL_SECS must be whole seconds, got {raw:?}"))?;
    if secs < MIN_INDEXER_INTERVAL_SECS {
        anyhow::bail!(
            "INDEXER_INTERVAL_SECS must be at least {MIN_INDEXER_INTERVAL_SECS}, got {secs}"
        );
    }
    Ok(Duration::from_secs(secs))
}

/// Parses `VILLAGE_STALE_AFTER_SECS`. Default: 2 x the indexer interval, so one
/// missed fetch is tolerated and two in a row make the village stale (Option A).
///
/// It must be longer than the interval: otherwise every village would turn
/// stale between two normal fetches and agents could never move funds.
fn parse_stale_after(value: Option<String>, indexer_interval: Duration) -> Result<Duration> {
    let stale_after = match value {
        None => indexer_interval * 2,
        Some(raw) => Duration::from_secs(raw.trim().parse().with_context(|| {
            format!("VILLAGE_STALE_AFTER_SECS must be whole seconds, got {raw:?}")
        })?),
    };
    if stale_after <= indexer_interval {
        anyhow::bail!(
            "VILLAGE_STALE_AFTER_SECS ({}s) must be longer than INDEXER_INTERVAL_SECS ({}s)",
            stale_after.as_secs(),
            indexer_interval.as_secs()
        );
    }
    Ok(stale_after)
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
            indexer_interval: Duration::from_secs(3600),
            village_stale_after: Duration::from_secs(7200),
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

    const HOUR: Duration = Duration::from_secs(3600);

    #[test]
    fn indexer_interval_defaults_to_one_hour() {
        assert_eq!(parse_indexer_interval(None).unwrap(), HOUR);
    }

    #[test]
    fn indexer_interval_accepts_a_day() {
        let day = parse_indexer_interval(Some("86400".to_string())).unwrap();
        assert_eq!(day, Duration::from_secs(86_400));
    }

    #[test]
    fn indexer_interval_rejects_too_small_or_invalid_values() {
        assert!(parse_indexer_interval(Some("59".to_string())).is_err());
        assert!(parse_indexer_interval(Some("1h".to_string())).is_err());
        assert!(parse_indexer_interval(Some("-5".to_string())).is_err());
    }

    #[test]
    fn stale_after_defaults_to_twice_the_interval() {
        assert_eq!(parse_stale_after(None, HOUR).unwrap(), HOUR * 2);
    }

    #[test]
    fn stale_after_follows_a_changed_interval() {
        let day = Duration::from_secs(86_400);
        assert_eq!(parse_stale_after(None, day).unwrap(), day * 2);
    }

    #[test]
    fn stale_after_must_be_longer_than_the_interval() {
        assert!(parse_stale_after(Some("3600".to_string()), HOUR).is_err());
        assert!(parse_stale_after(Some("1800".to_string()), HOUR).is_err());
        assert_eq!(
            parse_stale_after(Some("5400".to_string()), HOUR).unwrap(),
            Duration::from_secs(5400)
        );
    }
}
