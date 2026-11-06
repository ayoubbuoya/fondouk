//! DefiLlama yields API client: fetches a pool's full chart (history + latest point).
//!
//! Endpoint: `GET https://yields.llama.fi/chart/{pool_ref}` (free, no key, HANDOFF U1).
//! The chart holds about one point per day since the pool was listed, plus a
//! latest point refreshed during the day. Values are APY in percent (14.01 = 14.01%).
//!
//! Every point is returned, not only the newest one: storing them all is how the
//! backtest history gets imported (HANDOFF D22).

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

const BASE_URL: &str = "https://yields.llama.fi";

/// One reading of a pool, already converted to exact decimals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YieldPoint {
    pub ts: DateTime<Utc>,
    pub apy_base: Decimal,
    pub apy_reward: Decimal,
    pub tvl_usd: Decimal,
}

/// The usable points of one chart, oldest first, plus how many were skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chart {
    /// Sorted by timestamp, oldest first. Never empty (see `parse_chart`).
    pub points: Vec<YieldPoint>,
    /// Points dropped because they had a null `apyBase` or a number that does
    /// not fit a `Decimal`. Reported so the indexer can log it.
    pub skipped: usize,
}

impl Chart {
    /// The point with the latest timestamp.
    pub fn newest(&self) -> &YieldPoint {
        // `parse_chart` never builds an empty chart, so `last()` always exists.
        self.points
            .last()
            .expect("a parsed chart has at least one point")
    }
}

/// Shape of the `/chart/{pool}` response. Only the fields we use are listed;
/// serde ignores the rest (`apy`, `il7d`, ...).
#[derive(Debug, Deserialize)]
struct ChartResponse {
    status: String,
    data: Vec<RawPoint>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPoint {
    timestamp: DateTime<Utc>,
    tvl_usd: f64,
    /// Never null for our pools today, but typed as optional so one null point
    /// is skipped instead of failing the whole chart.
    apy_base: Option<f64>,
    /// Often null in real responses; null means "no reward", i.e. 0.
    apy_reward: Option<f64>,
}

/// Downloads the chart of one pool.
///
/// # Errors
/// Network failure, non-2xx status, or a body that `parse_chart` rejects.
pub async fn fetch_chart(http: &reqwest::Client, pool_ref: &str) -> Result<Chart> {
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

    parse_chart(&body).with_context(|| format!("unexpected response from {url}"))
}

/// Parses a `/chart/{pool}` body into its usable points, sorted oldest first.
///
/// Pure function (text in, value out) so it is unit-tested with a saved real
/// response, without network access. It does not trust the order of `data`.
///
/// A single bad point (null `apyBase`, number out of `Decimal` range) is
/// skipped and counted: one broken old point must not block the whole village.
///
/// # Errors
/// Invalid JSON, `status` other than `"success"`, empty `data`, or no usable
/// point at all. The indexer then leaves the village un-fetched, so it turns
/// stale on its own.
pub fn parse_chart(body: &str) -> Result<Chart> {
    let response: ChartResponse =
        serde_json::from_str(body).context("body is not valid chart JSON")?;
    if response.status != "success" {
        bail!("status is {:?}, expected \"success\"", response.status);
    }
    if response.data.is_empty() {
        bail!("chart has no data points");
    }

    let total = response.data.len();
    let mut points: Vec<YieldPoint> = response.data.into_iter().filter_map(to_point).collect();
    if points.is_empty() {
        bail!("chart has no usable data points ({total} skipped)");
    }
    points.sort_by_key(|point| point.ts);

    Ok(Chart {
        skipped: total - points.len(),
        points,
    })
}

/// Converts one raw point, or returns `None` when it is not usable.
fn to_point(raw: RawPoint) -> Option<YieldPoint> {
    Some(YieldPoint {
        ts: raw.timestamp,
        apy_base: to_decimal(raw.apy_base?)?,
        apy_reward: to_decimal(raw.apy_reward.unwrap_or(0.0))?,
        tvl_usd: to_decimal(raw.tvl_usd)?,
    })
}

/// Converts a JSON float to an exact decimal; `None` if it does not fit.
///
/// `Decimal::from_f64` picks the shortest decimal that round-trips, so `14.01014`
/// becomes exactly `14.01014`, not `14.0101399999...` (which is what the raw
/// binary float holds).
fn to_decimal(value: f64) -> Option<Decimal> {
    use rust_decimal::prelude::FromPrimitive;
    Decimal::from_f64(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Trimmed real response of the Rhea Lend USDC pool (2 Nov 2026): 3 points,
    /// with the newest one placed in the middle on purpose.
    const REAL_SAMPLE: &str = include_str!("fixtures/defillama_chart_lend_usdc.json");

    fn d(value: &str) -> Decimal {
        Decimal::from_str_exact(value).unwrap()
    }

    #[test]
    fn keeps_every_point_of_a_real_response() {
        let chart = parse_chart(REAL_SAMPLE).unwrap();

        assert_eq!(chart.points.len(), 3);
        assert_eq!(chart.skipped, 0);
    }

    #[test]
    fn sorts_points_oldest_first_whatever_the_input_order() {
        let chart = parse_chart(REAL_SAMPLE).unwrap();

        let apys: Vec<Decimal> = chart.points.iter().map(|p| p.apy_base).collect();
        assert_eq!(apys, vec![d("7.40248"), d("14.00627"), d("14.01014")]);
    }

    #[test]
    fn newest_is_the_latest_timestamp() {
        let chart = parse_chart(REAL_SAMPLE).unwrap();

        let expected_ts = Utc.with_ymd_and_hms(2026, 11, 2, 6, 1, 21).unwrap()
            + chrono::Duration::milliseconds(370);
        assert_eq!(chart.newest().ts, expected_ts);
        assert_eq!(chart.newest().apy_base, d("14.01014"));
        assert_eq!(chart.newest().tvl_usd, d("1625385"));
    }

    #[test]
    fn null_reward_means_zero() {
        let chart = parse_chart(REAL_SAMPLE).unwrap();
        assert!(chart.points.iter().all(|p| p.apy_reward == Decimal::ZERO));
    }

    #[test]
    fn a_null_base_apy_point_is_skipped_and_counted() {
        let body = r#"{"status":"success","data":[
            {"timestamp":"2026-11-01T00:00:00Z","tvlUsd":10,"apyBase":null,"apyReward":null},
            {"timestamp":"2026-11-02T00:00:00Z","tvlUsd":10,"apyBase":5.5,"apyReward":null}
        ]}"#;

        let chart = parse_chart(body).unwrap();

        assert_eq!(chart.skipped, 1);
        assert_eq!(chart.points.len(), 1);
        assert_eq!(chart.newest().apy_base, d("5.5"));
    }

    #[test]
    fn a_number_too_large_for_decimal_is_skipped() {
        let body = r#"{"status":"success","data":[
            {"timestamp":"2026-11-01T00:00:00Z","tvlUsd":1e300,"apyBase":5,"apyReward":null},
            {"timestamp":"2026-11-02T00:00:00Z","tvlUsd":10,"apyBase":6,"apyReward":null}
        ]}"#;

        let chart = parse_chart(body).unwrap();

        assert_eq!(chart.skipped, 1);
        assert_eq!(chart.newest().apy_base, d("6"));
    }

    #[test]
    fn rejects_a_chart_where_every_point_is_unusable() {
        let body = r#"{"status":"success","data":[
            {"timestamp":"2026-11-02T06:00:00Z","tvlUsd":10,"apyBase":null,"apyReward":null}
        ]}"#;
        let err = parse_chart(body).unwrap_err();
        assert!(err.to_string().contains("no usable data points"));
    }

    #[test]
    fn rejects_a_non_success_status() {
        let err = parse_chart(r#"{"status":"error","data":[]}"#).unwrap_err();
        assert!(err.to_string().contains("status"));
    }

    #[test]
    fn rejects_an_empty_chart() {
        let err = parse_chart(r#"{"status":"success","data":[]}"#).unwrap_err();
        assert!(err.to_string().contains("no data points"));
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_chart("<html>rate limited</html>").is_err());
    }
}
