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

/// Agent tick interval when `AGENT_TICK_SECS` is unset: 10 minutes (SPEC §3).
const DEFAULT_AGENT_TICK_SECS: u64 = 600;

/// Lowest accepted tick interval, so a typo cannot burn LLM credits.
const MIN_AGENT_TICK_SECS: u64 = 60;

/// NEAR RPC used when `NEAR_RPC_URL` is unset: FastNEAR's free testnet
/// endpoint. The old `rpc.testnet.near.org` is deprecated (NEAR docs, RPC
/// providers page, checked 6 Nov 2026). Testnet because the marketplace runs
/// there, so users log in with testnet accounts.
const DEFAULT_NEAR_RPC_URL: &str = "https://test.rpc.fastnear.com";

/// Marketplace contract used when `MARKETPLACE_CONTRACT_ID` is unset: the one
/// deployed on testnet on Day 8 (D57).
const DEFAULT_MARKETPLACE_CONTRACT_ID: &str = "marketplace.fondouk.testnet";

/// Where the LLM lives (D17). `Debug` is written by hand so the API key is
/// never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct LlmConfig {
    /// OpenAI-compatible base URL (`LLM_BASE_URL`), e.g. OpenRouter in dev,
    /// NEAR AI Cloud for the demo.
    pub base_url: String,
    /// `LLM_API_KEY`. Secret: never log it.
    pub api_key: String,
    /// `LLM_MODEL`, e.g. `openai/gpt-4o-mini`. Stored with every decision.
    pub model: String,
}

impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .field("model", &self.model)
            .finish()
    }
}

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
    /// LLM settings, or `None` when none of the `LLM_*` variables are set:
    /// the backend still runs (indexer, API), only the agent tick loop is off.
    pub llm: Option<LlmConfig>,
    /// Time between two agent ticks (`AGENT_TICK_SECS`, default 10 minutes).
    pub agent_tick_interval: Duration,
    /// NEAR JSON-RPC endpoint (`NEAR_RPC_URL`). Login uses it to check that
    /// the signing key belongs to the account (D38).
    pub near_rpc_url: String,
    /// Marketplace contract account (`MARKETPLACE_CONTRACT_ID`). Only events
    /// emitted by this account count as a clone purchase (F11.3).
    pub marketplace_contract_id: String,
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
            llm: parse_llm(
                std::env::var("LLM_BASE_URL").ok(),
                std::env::var("LLM_API_KEY").ok(),
                std::env::var("LLM_MODEL").ok(),
            )?,
            agent_tick_interval: parse_agent_tick(std::env::var("AGENT_TICK_SECS").ok())?,
            near_rpc_url: parse_near_rpc_url(std::env::var("NEAR_RPC_URL").ok())?,
            marketplace_contract_id: parse_marketplace_contract_id(
                std::env::var("MARKETPLACE_CONTRACT_ID").ok(),
            ),
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
            .field("llm", &self.llm)
            .field("agent_tick_interval", &self.agent_tick_interval)
            .field("near_rpc_url", &self.near_rpc_url)
            .field("marketplace_contract_id", &self.marketplace_contract_id)
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

/// The three `LLM_*` variables go together: all unset means "no LLM" (agent
/// loop off), all set means "use it". A partial setup is an error, so a
/// forgotten key fails at startup instead of silently disabling the agents.
/// Values are never echoed in errors (the key is secret).
fn parse_llm(
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
) -> Result<Option<LlmConfig>> {
    let present = |value: &Option<String>| value.as_deref().is_some_and(|v| !v.trim().is_empty());
    let missing: Vec<&str> = [
        ("LLM_BASE_URL", present(&base_url)),
        ("LLM_API_KEY", present(&api_key)),
        ("LLM_MODEL", present(&model)),
    ]
    .into_iter()
    .filter(|(_, is_set)| !is_set)
    .map(|(name, _)| name)
    .collect();

    match missing.len() {
        3 => Ok(None),
        0 => Ok(Some(LlmConfig {
            base_url: base_url.unwrap_or_default().trim().to_string(),
            api_key: api_key.unwrap_or_default().trim().to_string(),
            model: model.unwrap_or_default().trim().to_string(),
        })),
        _ => anyhow::bail!(
            "LLM settings are incomplete, missing: {} (set all three or none)",
            missing.join(", ")
        ),
    }
}

/// Parses `AGENT_TICK_SECS`. Default 10 minutes, minimum 60 s.
fn parse_agent_tick(value: Option<String>) -> Result<Duration> {
    let Some(raw) = value else {
        return Ok(Duration::from_secs(DEFAULT_AGENT_TICK_SECS));
    };
    let secs: u64 = raw
        .trim()
        .parse()
        .with_context(|| format!("AGENT_TICK_SECS must be whole seconds, got {raw:?}"))?;
    if secs < MIN_AGENT_TICK_SECS {
        anyhow::bail!("AGENT_TICK_SECS must be at least {MIN_AGENT_TICK_SECS}, got {secs}");
    }
    Ok(Duration::from_secs(secs))
}

/// Parses `NEAR_RPC_URL`, falling back to the FastNEAR testnet endpoint.
/// Only `http(s)://` URLs are accepted, so a typo fails at startup instead of
/// at the first login.
fn parse_near_rpc_url(value: Option<String>) -> Result<String> {
    let raw = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_NEAR_RPC_URL.to_string());
    if !(raw.starts_with("https://") || raw.starts_with("http://")) {
        anyhow::bail!("NEAR_RPC_URL must start with https:// or http://, got {raw:?}");
    }
    Ok(raw)
}

/// Parses `MARKETPLACE_CONTRACT_ID`, falling back to the testnet deployment.
/// A blank value counts as unset.
fn parse_marketplace_contract_id(value: Option<String>) -> String {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_MARKETPLACE_CONTRACT_ID.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marketplace_contract_defaults_to_the_testnet_deployment() {
        assert_eq!(
            parse_marketplace_contract_id(None),
            DEFAULT_MARKETPLACE_CONTRACT_ID
        );
        assert_eq!(
            parse_marketplace_contract_id(Some("  ".to_string())),
            DEFAULT_MARKETPLACE_CONTRACT_ID
        );
        assert_eq!(
            parse_marketplace_contract_id(Some(" other.testnet ".to_string())),
            "other.testnet"
        );
    }

    #[test]
    fn near_rpc_url_defaults_to_testnet_and_rejects_non_http() {
        assert_eq!(parse_near_rpc_url(None).unwrap(), DEFAULT_NEAR_RPC_URL);
        assert_eq!(
            parse_near_rpc_url(Some(" https://rpc.mainnet.fastnear.com ".to_string())).unwrap(),
            "https://rpc.mainnet.fastnear.com"
        );
        assert!(parse_near_rpc_url(Some("rpc.example".to_string())).is_err());
    }

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
            llm: Some(LlmConfig {
                base_url: "https://openrouter.ai/api/v1".to_string(),
                api_key: "sk-secret-key".to_string(),
                model: "openai/gpt-4o-mini".to_string(),
            }),
            agent_tick_interval: Duration::from_secs(600),
            near_rpc_url: DEFAULT_NEAR_RPC_URL.to_string(),
            marketplace_contract_id: DEFAULT_MARKETPLACE_CONTRACT_ID.to_string(),
        };

        let printed = format!("{config:?}");

        assert!(!printed.contains("secret-pw"));
        assert!(!printed.contains("sk-secret-key"));
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

    fn some(value: &str) -> Option<String> {
        Some(value.to_string())
    }

    #[test]
    fn llm_is_off_when_no_variable_is_set() {
        assert_eq!(parse_llm(None, None, None).unwrap(), None);
        assert_eq!(parse_llm(some(""), some(" "), None).unwrap(), None);
    }

    #[test]
    fn llm_is_on_when_all_three_are_set() {
        let llm = parse_llm(some("https://x/v1"), some("key"), some("m"))
            .unwrap()
            .unwrap();
        assert_eq!(llm.model, "m");
    }

    #[test]
    fn partial_llm_settings_name_the_missing_ones_but_not_the_key() {
        let err = parse_llm(some("https://x/v1"), some("sk-secret"), None).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("LLM_MODEL"));
        assert!(!message.contains("sk-secret"));
    }

    #[test]
    fn agent_tick_defaults_to_ten_minutes_and_has_a_minimum() {
        assert_eq!(parse_agent_tick(None).unwrap(), Duration::from_secs(600));
        assert!(parse_agent_tick(some("59")).is_err());
        assert!(parse_agent_tick(some("10m")).is_err());
        assert_eq!(
            parse_agent_tick(some("60")).unwrap(),
            Duration::from_secs(60)
        );
    }
}
