//! yfinance-style Ticker API for forex data.

use crate::core::client::DukascopyClient;
use crate::error::DukascopyError;
use crate::models::{CurrencyExchange, CurrencyPair};
use chrono::{DateTime, Duration, Utc};
use std::str::FromStr;

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

    // ==================== Data Fetching ====================

    /// Fetches the exchange rate at a specific timestamp.
    pub async fn rate_at(
        &self,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        DukascopyClient::get_exchange_rate(&self.pair, timestamp).await
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
        let duration = parse_period(period)?;
        let end = Utc::now() - Duration::hours(1);
        let start = end - duration;
        DukascopyClient::get_exchange_rates_range(&self.pair, start, end, self.interval).await
    }

    /// Fetches historical data between two dates.
    pub async fn history_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        DukascopyClient::get_exchange_rates_range(&self.pair, start, end, self.interval).await
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

    Ok(match unit {
        "d" => Duration::days(num),
        "w" => Duration::weeks(num),
        "mo" => Duration::days(num * 30),
        "y" => Duration::days(num * 365),
        _ => unreachable!(),
    })
}

// ============================================================================
// Batch Download
// ============================================================================

/// Downloads historical data for multiple tickers.
pub async fn download(
    tickers: &[Ticker],
    period: &str,
) -> Result<Vec<(Ticker, Vec<CurrencyExchange>)>, DukascopyError> {
    let mut results = Vec::with_capacity(tickers.len());
    for ticker in tickers {
        let history = ticker.history(period).await?;
        results.push((ticker.clone(), history));
    }
    Ok(results)
}

/// Downloads historical data with custom date range.
pub async fn download_range(
    tickers: &[Ticker],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<(Ticker, Vec<CurrencyExchange>)>, DukascopyError> {
    let mut results = Vec::with_capacity(tickers.len());
    for ticker in tickers {
        let history = ticker.history_range(start, end).await?;
        results.push((ticker.clone(), history));
    }
    Ok(results)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_ticker_interval() {
        let ticker = Ticker::new("EUR", "USD").interval(Duration::minutes(30));
        assert_eq!(ticker.interval, Duration::minutes(30));
    }
}
