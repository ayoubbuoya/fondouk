//! DefiLlama yields API client: fetches a pool's chart and extracts the newest point.
//!
//! Endpoint: `GET https://yields.llama.fi/chart/{pool_ref}` (free, no key, HANDOFF U1).
//! Values are APY in percent (14.01 = 14.01%).

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

const BASE_URL: &str = "https://yields.llama.fi";

/// The newest reading of one pool, already converted to exact decimals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestPoint {
    pub ts: DateTime<Utc>,
    pub apy_base: Decimal,
    pub apy_reward: Decimal,
    pub tvl_usd: Decimal,
}

/// Shape of the `/chart/{pool}` response. Only the fields we use are listed;
/// serde ignores the rest (`apy`, `il7d`, ...).
#[derive(Debug, Deserialize)]
struct ChartResponse {
    status: String,
    data: Vec<ChartPoint>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChartPoint {
    timestamp: DateTime<Utc>,
    tvl_usd: f64,
    /// Never null for our pools today, but typed as optional so a null becomes
    /// a clear error instead of a confusing parse failure.
    apy_base: Option<f64>,
    /// Often null in real responses; null means "no reward", i.e. 0.
    apy_reward: Option<f64>,
}

/// Downloads the chart of one pool and returns its newest point.
///
/// # Errors
/// Network failure, non-2xx status, or a body that `parse_latest_point` rejects.
pub async fn fetch_latest_point(http: &reqwest::Client, pool_ref: &str) -> Result<LatestPoint> {
    let url = format!("{BASE_URL}/chart/{pool_ref}");
    let body = http
        .get(&url)
        .send()
        .await
        .with_context(|| format!("request to {url} failed"))?
        .error_for_status()
        .with_context(|| format!("{url} returned an error status"))?
        .text()
        .await
        .with_context(|| format!("could not read the body of {url}"))?;

    parse_latest_point(&body).with_context(|| format!("unexpected response from {url}"))
}

/// Parses a `/chart/{pool}` body and returns the point with the latest timestamp.
///
/// Pure function (text in, value out) so it is unit-tested with a saved real
/// response, without network access. It does not trust the order of `data`.
///
/// # Errors
/// Invalid JSON, `status` other than `"success"`, empty `data`, a null
/// `apyBase`, or a number that is not finite.
pub fn parse_latest_point(body: &str) -> Result<LatestPoint> {
    let response: ChartResponse =
        serde_json::from_str(body).context("body is not valid chart JSON")?;
    if response.status != "success" {
        bail!("status is {:?}, expected \"success\"", response.status);
    }

    let newest = response
        .data
        .into_iter()
        .max_by_key(|point| point.timestamp)
        .context("chart has no data points")?;

    let apy_base = newest
        .apy_base
        .with_context(|| format!("apyBase is null at {}", newest.timestamp))?;

    Ok(LatestPoint {
        ts: newest.timestamp,
        apy_base: to_decimal(apy_base, "apyBase")?,
        apy_reward: to_decimal(newest.apy_reward.unwrap_or(0.0), "apyReward")?,
        tvl_usd: to_decimal(newest.tvl_usd, "tvlUsd")?,
    })
}

/// Converts a JSON float to an exact decimal.
///
/// `Decimal::from_f64` picks the shortest decimal that round-trips, so `14.01014`
/// becomes exactly `14.01014`, not `14.0101399999...` (which is what the raw
/// binary float holds).
fn to_decimal(value: f64, field: &str) -> Result<Decimal> {
    use rust_decimal::prelude::FromPrimitive;
    Decimal::from_f64(value).with_context(|| format!("{field} is not a finite number: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Trimmed real response of the Rhea Lend USDC pool (2 Nov 2026), with the
    /// newest point placed in the middle on purpose.
    const REAL_SAMPLE: &str = include_str!("fixtures/defillama_chart_lend_usdc.json");

    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    #[test]
    fn picks_the_newest_point_from_a_real_response() {
        let point = parse_latest_point(REAL_SAMPLE).unwrap();

        let expected_ts = Utc.with_ymd_and_hms(2026, 11, 2, 6, 1, 21).unwrap()
            + chrono::Duration::milliseconds(370);
        assert_eq!(point.ts, expected_ts);
        assert_eq!(point.apy_base, d("14.01014"));
        assert_eq!(point.tvl_usd, d("1625385"));
    }

    #[test]
    fn null_reward_means_zero() {
        let point = parse_latest_point(REAL_SAMPLE).unwrap();
        assert_eq!(point.apy_reward, Decimal::ZERO);
    }

    #[test]
    fn rejects_a_non_success_status() {
        let err = parse_latest_point(r#"{"status":"error","data":[]}"#).unwrap_err();
        assert!(err.to_string().contains("status"));
    }

    #[test]
    fn rejects_an_empty_chart() {
        let err = parse_latest_point(r#"{"status":"success","data":[]}"#).unwrap_err();
        assert!(err.to_string().contains("no data points"));
    }

    #[test]
    fn rejects_a_null_base_apy() {
        let body = r#"{"status":"success","data":[
            {"timestamp":"2026-11-02T06:00:00Z","tvlUsd":10,"apyBase":null,"apyReward":null}
        ]}"#;
        let err = parse_latest_point(body).unwrap_err();
        assert!(err.to_string().contains("apyBase is null"));
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_latest_point("<html>rate limited</html>").is_err());
    }
}
