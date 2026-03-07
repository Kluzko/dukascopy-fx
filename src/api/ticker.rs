//! yfinance-style Ticker API for forex data.

use crate::core::client::{ConfiguredClient, DukascopyClient};
use crate::error::DukascopyError;
use crate::market::last_available_tick_time;
use crate::models::{CurrencyExchange, CurrencyPair};
use crate::storage::checkpoint::CheckpointStore;
use chrono::{DateTime, Duration, Utc};
use futures::stream::{self, StreamExt, TryStreamExt};
use std::str::FromStr;

const DEFAULT_DOWNLOAD_CONCURRENCY: usize = 8;

/// Typed period for historical queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Days(i64),
    Weeks(i64),
    Months(i64),
    Years(i64),
}

impl Period {
    pub fn to_duration(self) -> Result<Duration, DukascopyError> {
        let (value, unit) = match self {
            Self::Days(value) => (value, "d"),
            Self::Weeks(value) => (value, "w"),
            Self::Months(value) => (value, "mo"),
            Self::Years(value) => (value, "y"),
        };

        if value <= 0 {
            return Err(DukascopyError::InvalidRequest(
                "Period must be positive".to_string(),
            ));
        }

        Ok(match unit {
            "d" => Duration::days(value),
            "w" => Duration::weeks(value),
            "mo" => Duration::days(value * 30),
            "y" => Duration::days(value * 365),
            _ => unreachable!("validated period unit"),
        })
    }
}

impl FromStr for Period {
    type Err = DukascopyError;

    fn from_str(period: &str) -> Result<Self, Self::Err> {
        let period = period.trim().to_lowercase();

        let (num_str, unit) = if period.ends_with("mo") {
            (&period[..period.len() - 2], "mo")
        } else if period.ends_with('d') {
            (&period[..period.len() - 1], "d")
        } else if period.ends_with('w') {
            (&period[..period.len() - 1], "w")
        } else if period.ends_with('y') {
            (&period[..period.len() - 1], "y")
        } else {
            return Err(DukascopyError::InvalidRequest(format!(
                "Invalid period format: '{}'. Use '1d', '1w', '1mo', '1y'",
                period
            )));
        };

        let num: i64 = num_str.parse().map_err(|_| {
            DukascopyError::InvalidRequest(format!("Invalid period number in '{}'", period))
        })?;

        if num <= 0 {
            return Err(DukascopyError::InvalidRequest(
                "Period must be positive".to_string(),
            ));
        }

        match unit {
            "d" => Ok(Self::Days(num)),
            "w" => Ok(Self::Weeks(num)),
            "mo" => Ok(Self::Months(num)),
            "y" => Ok(Self::Years(num)),
            _ => unreachable!("validated period unit"),
        }
    }
}

/// A forex ticker for fetching exchange rate data.
///
/// # Example
///
/// ```no_run
/// use dukascopy_fx::Ticker;
///
/// # async fn example() -> dukascopy_fx::Result<()> {
/// let ticker = Ticker::new("EUR", "USD");
///
/// // Get recent rate
/// let rate = ticker.rate().await?;
/// println!("EUR/USD: {}", rate.rate);
///
/// // Get historical data
/// let history = ticker.history("1w").await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Ticker {
    pair: CurrencyPair,
    interval: Duration,
}

impl Ticker {
    /// Creates a new ticker for a currency pair with validation.
    pub fn try_new(from: &str, to: &str) -> Result<Self, DukascopyError> {
        Ok(Self {
            pair: CurrencyPair::try_new(from, to)?,
            interval: Duration::hours(1),
        })
    }

    /// Creates a new ticker for a currency pair.
    #[inline]
    pub fn new(from: &str, to: &str) -> Self {
        Self {
            pair: CurrencyPair::new(from, to),
            interval: Duration::hours(1),
        }
    }

    /// Creates a ticker from a pair string like "EUR/USD" or "EURUSD".
    pub fn parse(pair: &str) -> Result<Self, DukascopyError> {
        let currency_pair: CurrencyPair = pair.parse()?;
        Ok(Self {
            pair: currency_pair,
            interval: Duration::hours(1),
        })
    }

    /// Sets the data interval for historical queries.
    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Returns the currency pair.
    #[inline]
    pub fn pair(&self) -> &CurrencyPair {
        &self.pair
    }

    /// Returns the ticker symbol (e.g., "EURUSD").
    #[inline]
    pub fn symbol(&self) -> String {
        self.pair.as_symbol()
    }

    /// Returns sampling interval used by this ticker.
    #[inline]
    pub fn interval_value(&self) -> Duration {
        self.interval
    }

    // ==================== Data Fetching ====================

    /// Fetches the exchange rate at a specific timestamp.
    pub async fn rate_at(
        &self,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        DukascopyClient::get_exchange_rate(&self.pair, timestamp).await
    }

    /// Fetches the exchange rate at a specific timestamp using a configured client.
    pub async fn rate_at_with_client(
        &self,
        client: &ConfiguredClient,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        client.get_exchange_rate(&self.pair, timestamp).await
    }

    /// Fetches the most recent available exchange rate.
    pub async fn rate(&self) -> Result<CurrencyExchange, DukascopyError> {
        let timestamp = Utc::now() - Duration::hours(1);
        self.rate_at(timestamp).await
    }

    /// Fetches historical data for a time period.
    ///
    /// # Period Strings
    /// - `"1d"` - 1 day
    /// - `"5d"` - 5 days
    /// - `"1w"` - 1 week
    /// - `"1mo"` - 1 month (30 days)
    /// - `"3mo"` - 3 months
    /// - `"1y"` - 1 year
    pub async fn history(&self, period: &str) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let end = Utc::now() - Duration::hours(1);
        self.history_from_end(period, end).await
    }

    /// Fetches historical data using typed period.
    pub async fn history_period(
        &self,
        period: Period,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let end = Utc::now() - Duration::hours(1);
        self.history_period_from_end(period, end).await
    }

    /// Fetches historical data for a time period using a configured client.
    pub async fn history_with_client(
        &self,
        client: &ConfiguredClient,
        period: &str,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let end = Utc::now() - Duration::hours(1);
        self.history_from_end_with_client(client, period, end).await
    }

    /// Fetches historical data for a time period ending at a specific timestamp.
    pub async fn history_from_end(
        &self,
        period: &str,
        end: DateTime<Utc>,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let start = end - parse_period(period)?;
        self.history_range(start, end).await
    }

    /// Fetches historical data for a typed period ending at a specific timestamp.
    pub async fn history_period_from_end(
        &self,
        period: Period,
        end: DateTime<Utc>,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let start = end - period.to_duration()?;
        self.history_range(start, end).await
    }

    /// Fetches historical data for a time period ending at a specific timestamp using a configured client.
    pub async fn history_from_end_with_client(
        &self,
        client: &ConfiguredClient,
        period: &str,
        end: DateTime<Utc>,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let start = end - parse_period(period)?;
        client
            .get_exchange_rates_range(&self.pair, start, end, self.interval)
            .await
    }

    /// Fetches historical data for a typed period ending at a specific timestamp using a configured client.
    pub async fn history_period_from_end_with_client(
        &self,
        client: &ConfiguredClient,
        period: Period,
        end: DateTime<Utc>,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let start = end - period.to_duration()?;
        client
            .get_exchange_rates_range(&self.pair, start, end, self.interval)
            .await
    }

    /// Fetches historical data between two dates.
    pub async fn history_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        DukascopyClient::get_exchange_rates_range(&self.pair, start, end, self.interval).await
    }

    /// Fetches historical data between two dates using a configured client.
    pub async fn history_range_with_client(
        &self,
        client: &ConfiguredClient,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        client
            .get_exchange_rates_range(&self.pair, start, end, self.interval)
            .await
    }

    /// Incrementally fetches newly available data and updates checkpoint.
    ///
    /// If checkpoint is missing, uses `lookback` as initial backfill range.
    pub async fn fetch_incremental<S: CheckpointStore>(
        &self,
        store: &S,
        lookback: Duration,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let end = last_available_tick_time(Utc::now() - Duration::hours(1));
        self.fetch_incremental_at(store, lookback, end).await
    }

    /// Incrementally fetches newly available data using a configured client and updates checkpoint.
    pub async fn fetch_incremental_with_client<S: CheckpointStore>(
        &self,
        client: &ConfiguredClient,
        store: &S,
        lookback: Duration,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let end = last_available_tick_time(Utc::now() - Duration::hours(1));
        self.fetch_incremental_with_client_at(client, store, lookback, end)
            .await
    }

    /// Incrementally fetches data up to a provided end timestamp.
    pub async fn fetch_incremental_at<S: CheckpointStore>(
        &self,
        store: &S,
        lookback: Duration,
        end: DateTime<Utc>,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        self.fetch_incremental_with_fetch_fn(store, lookback, end, |start, range_end| async move {
            self.history_range(start, range_end).await
        })
        .await
    }

    /// Incrementally fetches data up to a provided end timestamp using a configured client.
    pub async fn fetch_incremental_with_client_at<S: CheckpointStore>(
        &self,
        client: &ConfiguredClient,
        store: &S,
        lookback: Duration,
        end: DateTime<Utc>,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        self.fetch_incremental_with_fetch_fn(store, lookback, end, |start, range_end| async move {
            self.history_range_with_client(client, start, range_end)
                .await
        })
        .await
    }

    async fn fetch_incremental_with_fetch_fn<S, F, Fut>(
        &self,
        store: &S,
        lookback: Duration,
        end: DateTime<Utc>,
        fetch_fn: F,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError>
    where
        S: CheckpointStore,
        F: Fn(DateTime<Utc>, DateTime<Utc>) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<CurrencyExchange>, DukascopyError>>,
    {
        if self.interval <= Duration::zero() {
            return Err(DukascopyError::InvalidRequest(
                "Interval must be a positive duration".to_string(),
            ));
        }

        if lookback <= Duration::zero() {
            return Err(DukascopyError::InvalidRequest(
                "Lookback must be a positive duration".to_string(),
            ));
        }

        let checkpoint_key = self.checkpoint_key();
        let end = last_available_tick_time(end);
        let retry_buffer = self.interval + self.interval;
        let start = match store.get(&checkpoint_key)? {
            Some(last_timestamp) => last_timestamp - retry_buffer,
            None => end - lookback,
        };

        if start >= end {
            return Ok(Vec::new());
        }

        let rates = fetch_fn(start, end).await?;
        let rates = deduplicate_by_timestamp(rates);

        if let Some(last) = rates.last() {
            store.set(&checkpoint_key, last.timestamp)?;
        }

        Ok(rates)
    }

    /// Returns a stable checkpoint key for this ticker.
    #[inline]
    pub fn checkpoint_key(&self) -> String {
        format!("{}:{}", self.symbol(), self.interval.num_seconds())
    }

    // ==================== Convenience Constructors ====================

    #[inline]
    pub fn eur_usd() -> Self {
        Self::new("EUR", "USD")
    }
    #[inline]
    pub fn gbp_usd() -> Self {
        Self::new("GBP", "USD")
    }
    #[inline]
    pub fn usd_jpy() -> Self {
        Self::new("USD", "JPY")
    }
    #[inline]
    pub fn usd_chf() -> Self {
        Self::new("USD", "CHF")
    }
    #[inline]
    pub fn aud_usd() -> Self {
        Self::new("AUD", "USD")
    }
    #[inline]
    pub fn usd_cad() -> Self {
        Self::new("USD", "CAD")
    }
    #[inline]
    pub fn xau_usd() -> Self {
        Self::new("XAU", "USD")
    }
    #[inline]
    pub fn xag_usd() -> Self {
        Self::new("XAG", "USD")
    }
}

impl FromStr for Ticker {
    type Err = DukascopyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ticker::parse(s)
    }
}

// ============================================================================
// Period Parsing
// ============================================================================

fn parse_period(period: &str) -> Result<Duration, DukascopyError> {
    Period::from_str(period)?.to_duration()
}

// ============================================================================
// Batch Download
// ============================================================================

/// Downloads historical data for multiple tickers.
pub async fn download(
    tickers: &[Ticker],
    period: &str,
) -> Result<Vec<(Ticker, Vec<CurrencyExchange>)>, DukascopyError> {
    if tickers.is_empty() {
        return Ok(Vec::new());
    }

    let concurrency = tickers.len().clamp(1, DEFAULT_DOWNLOAD_CONCURRENCY);
    let period = period.to_string();
    let mut indexed_results: Vec<(usize, Ticker, Vec<CurrencyExchange>)> =
        stream::iter(tickers.iter().cloned().enumerate().map(|(index, ticker)| {
            let period = period.clone();
            async move {
                let history = ticker.history(&period).await?;
                Ok::<_, DukascopyError>((index, ticker, history))
            }
        }))
        .buffer_unordered(concurrency)
        .try_collect()
        .await?;

    indexed_results.sort_by_key(|(index, _, _)| *index);
    Ok(indexed_results
        .into_iter()
        .map(|(_, ticker, history)| (ticker, history))
        .collect())
}

/// Downloads historical data with custom date range.
pub async fn download_range(
    tickers: &[Ticker],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<(Ticker, Vec<CurrencyExchange>)>, DukascopyError> {
    if tickers.is_empty() {
        return Ok(Vec::new());
    }

    let concurrency = tickers.len().clamp(1, DEFAULT_DOWNLOAD_CONCURRENCY);
    let mut indexed_results: Vec<(usize, Ticker, Vec<CurrencyExchange>)> = stream::iter(
        tickers
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, ticker)| async move {
                let history = ticker.history_range(start, end).await?;
                Ok::<_, DukascopyError>((index, ticker, history))
            }),
    )
    .buffer_unordered(concurrency)
    .try_collect()
    .await?;

    indexed_results.sort_by_key(|(index, _, _)| *index);
    Ok(indexed_results
        .into_iter()
        .map(|(_, ticker, history)| (ticker, history))
        .collect())
}

/// Incrementally downloads data for multiple tickers using checkpoint store.
pub async fn download_incremental<S: CheckpointStore>(
    tickers: &[Ticker],
    store: &S,
    lookback: Duration,
) -> Result<Vec<(Ticker, Vec<CurrencyExchange>)>, DukascopyError> {
    if tickers.is_empty() {
        return Ok(Vec::new());
    }

    let concurrency = tickers.len().clamp(1, DEFAULT_DOWNLOAD_CONCURRENCY);
    let mut indexed_results: Vec<(usize, Ticker, Vec<CurrencyExchange>)> = stream::iter(
        tickers
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, ticker)| async move {
                let history = ticker.fetch_incremental(store, lookback).await?;
                Ok::<_, DukascopyError>((index, ticker, history))
            }),
    )
    .buffer_unordered(concurrency)
    .try_collect()
    .await?;

    indexed_results.sort_by_key(|(index, _, _)| *index);
    Ok(indexed_results
        .into_iter()
        .map(|(_, ticker, history)| (ticker, history))
        .collect())
}

fn deduplicate_by_timestamp(mut history: Vec<CurrencyExchange>) -> Vec<CurrencyExchange> {
    history.sort_by_key(|rate| rate.timestamp);
    history.dedup_by_key(|rate| rate.timestamp);
    history
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct InMemoryCheckpointStore {
        data: Mutex<HashMap<String, DateTime<Utc>>>,
    }

    impl CheckpointStore for InMemoryCheckpointStore {
        fn get(&self, key: &str) -> Result<Option<DateTime<Utc>>, DukascopyError> {
            let data = self.data.lock().map_err(|err| {
                DukascopyError::Unknown(format!("Checkpoint lock poisoned: {}", err))
            })?;
            Ok(data.get(key).cloned())
        }

        fn set(&self, key: &str, timestamp: DateTime<Utc>) -> Result<(), DukascopyError> {
            let mut data = self.data.lock().map_err(|err| {
                DukascopyError::Unknown(format!("Checkpoint lock poisoned: {}", err))
            })?;
            data.insert(key.to_string(), timestamp);
            Ok(())
        }
    }

    fn sample_exchange(ts: DateTime<Utc>) -> CurrencyExchange {
        CurrencyExchange {
            pair: CurrencyPair::new("EUR", "USD"),
            rate: Decimal::from_str("1.10000").unwrap(),
            timestamp: ts,
            ask: Decimal::from_str("1.10010").unwrap(),
            bid: Decimal::from_str("1.09990").unwrap(),
            ask_volume: 1.0,
            bid_volume: 1.0,
        }
    }

    #[test]
    fn test_ticker_new() {
        let ticker = Ticker::new("EUR", "USD");
        assert_eq!(ticker.symbol(), "EURUSD");
    }

    #[test]
    fn test_ticker_parse() {
        let ticker = Ticker::parse("EUR/USD").unwrap();
        assert_eq!(ticker.symbol(), "EURUSD");

        let ticker = Ticker::parse("USDJPY").unwrap();
        assert_eq!(ticker.symbol(), "USDJPY");

        let ticker = Ticker::parse("AAPL/USD").unwrap();
        assert_eq!(ticker.symbol(), "AAPLUSD");
    }

    #[test]
    fn test_from_str() {
        let ticker: Ticker = "EUR/USD".parse().unwrap();
        assert_eq!(ticker.symbol(), "EURUSD");
    }

    #[test]
    fn test_convenience_constructors() {
        assert_eq!(Ticker::eur_usd().symbol(), "EURUSD");
        assert_eq!(Ticker::usd_jpy().symbol(), "USDJPY");
        assert_eq!(Ticker::xau_usd().symbol(), "XAUUSD");
    }

    #[test]
    fn test_parse_period() {
        assert_eq!(parse_period("1d").unwrap(), Duration::days(1));
        assert_eq!(parse_period("5d").unwrap(), Duration::days(5));
        assert_eq!(parse_period("1w").unwrap(), Duration::weeks(1));
        assert_eq!(parse_period("1mo").unwrap(), Duration::days(30));
        assert_eq!(parse_period("1y").unwrap(), Duration::days(365));
        assert_eq!(parse_period("1D").unwrap(), Duration::days(1));
    }

    #[test]
    fn test_parse_period_invalid() {
        assert!(parse_period("abc").is_err());
        assert!(parse_period("0d").is_err());
        assert!(parse_period("-1d").is_err());
    }

    #[test]
    fn test_period_from_str() {
        assert_eq!(Period::from_str("1d").unwrap(), Period::Days(1));
        assert_eq!(Period::from_str("2w").unwrap(), Period::Weeks(2));
        assert_eq!(Period::from_str("3mo").unwrap(), Period::Months(3));
        assert_eq!(Period::from_str("1y").unwrap(), Period::Years(1));
    }

    #[test]
    fn test_period_from_str_invalid() {
        assert!(Period::from_str("bad").is_err());
        assert!(Period::from_str("0d").is_err());
        assert!(Period::from_str("-1d").is_err());
        assert!(Period::Days(0).to_duration().is_err());
        assert!(Period::Weeks(-1).to_duration().is_err());
    }

    #[test]
    fn test_ticker_interval() {
        let ticker = Ticker::new("EUR", "USD").interval(Duration::minutes(30));
        assert_eq!(ticker.interval, Duration::minutes(30));
    }

    #[test]
    fn test_ticker_try_new_validates_input() {
        let ticker = Ticker::try_new("eur", "usd").unwrap();
        assert_eq!(ticker.symbol(), "EURUSD");

        let err = Ticker::try_new("BAD$", "USD").unwrap_err();
        assert!(matches!(
            err,
            DukascopyError::InvalidCurrencyCode { code, .. } if code == "BAD$"
        ));
    }

    #[test]
    fn test_checkpoint_key() {
        let ticker = Ticker::new("EUR", "USD").interval(Duration::minutes(30));
        assert_eq!(ticker.checkpoint_key(), "EURUSD:1800");
    }

    #[tokio::test]
    async fn test_fetch_incremental_at_uses_lookback_without_checkpoint() {
        let store = InMemoryCheckpointStore::default();
        let ticker = Ticker::new("EUR", "USD").interval(Duration::hours(1));
        let end = Utc.with_ymd_and_hms(2025, 1, 10, 10, 0, 0).unwrap();

        let observed = Arc::new(Mutex::new(None::<(DateTime<Utc>, DateTime<Utc>)>));
        let observed_clone = Arc::clone(&observed);
        let rows = ticker
            .fetch_incremental_with_fetch_fn(
                &store,
                Duration::hours(6),
                end,
                move |start, range_end| {
                    let observed_clone = Arc::clone(&observed_clone);
                    async move {
                        let mut slot = observed_clone.lock().unwrap();
                        *slot = Some((start, range_end));
                        Ok(Vec::new())
                    }
                },
            )
            .await
            .unwrap();

        assert!(rows.is_empty());
        let (start, range_end) = observed.lock().unwrap().unwrap();
        let expected_end = last_available_tick_time(end);
        assert_eq!(range_end, expected_end);
        assert_eq!(start, expected_end - Duration::hours(6));
    }

    #[tokio::test]
    async fn test_fetch_incremental_at_with_checkpoint_uses_retry_buffer() {
        let store = InMemoryCheckpointStore::default();
        let ticker = Ticker::new("EUR", "USD").interval(Duration::hours(1));
        let checkpoint_key = ticker.checkpoint_key();
        let checkpoint_ts = Utc.with_ymd_and_hms(2025, 1, 10, 8, 0, 0).unwrap();
        store.set(&checkpoint_key, checkpoint_ts).unwrap();

        let end = Utc.with_ymd_and_hms(2025, 1, 10, 10, 0, 0).unwrap();
        let observed = Arc::new(Mutex::new(None::<(DateTime<Utc>, DateTime<Utc>)>));
        let observed_clone = Arc::clone(&observed);

        let _ = ticker
            .fetch_incremental_with_fetch_fn(
                &store,
                Duration::hours(24),
                end,
                move |start, range_end| {
                    let observed_clone = Arc::clone(&observed_clone);
                    async move {
                        let mut slot = observed_clone.lock().unwrap();
                        *slot = Some((start, range_end));
                        Ok(Vec::new())
                    }
                },
            )
            .await
            .unwrap();

        let (start, _) = observed.lock().unwrap().unwrap();
        assert_eq!(start, checkpoint_ts - Duration::hours(2));
    }

    #[tokio::test]
    async fn test_fetch_incremental_at_deduplicates_and_updates_checkpoint() {
        let store = InMemoryCheckpointStore::default();
        let ticker = Ticker::new("EUR", "USD").interval(Duration::hours(1));
        let end = Utc.with_ymd_and_hms(2025, 1, 10, 10, 0, 0).unwrap();
        let first = Utc.with_ymd_and_hms(2025, 1, 10, 7, 0, 0).unwrap();
        let second = Utc.with_ymd_and_hms(2025, 1, 10, 8, 0, 0).unwrap();

        let rows = ticker
            .fetch_incremental_with_fetch_fn(
                &store,
                Duration::hours(4),
                end,
                move |_start, _end| async move {
                    Ok(vec![
                        sample_exchange(first),
                        sample_exchange(first),
                        sample_exchange(second),
                    ])
                },
            )
            .await
            .unwrap();

        assert_eq!(rows.len(), 2);
        let checkpoint = store.get(&ticker.checkpoint_key()).unwrap().unwrap();
        assert_eq!(checkpoint, second);
    }

    #[tokio::test]
    async fn test_fetch_incremental_at_rejects_non_positive_lookback() {
        let store = InMemoryCheckpointStore::default();
        let ticker = Ticker::new("EUR", "USD");
        let end = Utc.with_ymd_and_hms(2025, 1, 10, 10, 0, 0).unwrap();

        let result = ticker
            .fetch_incremental_with_fetch_fn(
                &store,
                Duration::zero(),
                end,
                |_start, _end| async move { Ok(Vec::new()) },
            )
            .await;

        assert!(matches!(result, Err(DukascopyError::InvalidRequest(_))));
    }

    #[tokio::test]
    async fn test_history_from_end_rejects_invalid_period_without_network_call() {
        let ticker = Ticker::new("EUR", "USD");
        let end = Utc.with_ymd_and_hms(2025, 1, 10, 10, 0, 0).unwrap();
        let result = ticker.history_from_end("bad", end).await;
        assert!(matches!(result, Err(DukascopyError::InvalidRequest(_))));
    }

    #[tokio::test]
    async fn test_history_period_from_end_rejects_non_positive_period_without_network_call() {
        let ticker = Ticker::new("EUR", "USD");
        let end = Utc.with_ymd_and_hms(2025, 1, 10, 10, 0, 0).unwrap();
        let result = ticker.history_period_from_end(Period::Days(0), end).await;
        assert!(matches!(result, Err(DukascopyError::InvalidRequest(_))));
    }

    #[tokio::test]
    async fn test_download_empty_returns_empty_without_network_call() {
        let result = download(&[], "1d").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_download_range_empty_returns_empty_without_network_call() {
        let start = Utc.with_ymd_and_hms(2025, 1, 10, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 1, 10, 1, 0, 0).unwrap();
        let result = download_range(&[], start, end).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_download_incremental_empty_returns_empty_without_network_call() {
        let store = InMemoryCheckpointStore::default();
        let result = download_incremental(&[], &store, Duration::hours(1))
            .await
            .unwrap();
        assert!(result.is_empty());
    }
}
