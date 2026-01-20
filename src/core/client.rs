//! HTTP client for fetching tick data from Dukascopy.

use crate::core::instrument::HasInstrumentConfig;
use crate::core::instrument::{InstrumentConfig, InstrumentProvider, OverrideInstrumentProvider};
use crate::core::parser::{DukascopyParser, ParsedTick, TICK_SIZE_BYTES};
use crate::error::DukascopyError;
use crate::market::{is_weekend, last_available_tick_time};
use crate::models::{CurrencyExchange, CurrencyPair};

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use log::{debug, info, warn};
use lru::LruCache;
use reqwest::Client;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use tokio::sync::OnceCell;

// ============================================================================
// Constants
// ============================================================================

/// Default LRU cache size for decompressed tick data
pub const DEFAULT_CACHE_SIZE: usize = 100;

/// Default maximum idle connections per host
pub const DEFAULT_MAX_IDLE_CONNECTIONS: usize = 10;

/// Default HTTP request timeout in seconds
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Dukascopy API base URL
pub const DUKASCOPY_BASE_URL: &str = "https://datafeed.dukascopy.com/datafeed";

/// Number of decimal places for rate rounding
const RATE_DECIMAL_PLACES: u32 = 4;

// Global default client instance
static DEFAULT_CLIENT: OnceCell<ConfiguredClient> = OnceCell::const_new();

/// Gets or initializes the global default client
async fn get_default_client() -> &'static ConfiguredClient {
    DEFAULT_CLIENT
        .get_or_init(|| async { DukascopyClientBuilder::new().build() })
        .await
}

// ============================================================================
// Client Configuration
// ============================================================================

/// Configuration for a Dukascopy client instance.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Cache size (number of hourly data files to cache)
    pub cache_size: usize,
    /// HTTP request timeout in seconds
    pub timeout_secs: u64,
    /// Maximum idle connections per host
    pub max_idle_connections: usize,
    /// Base URL for the Dukascopy API
    pub base_url: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            cache_size: DEFAULT_CACHE_SIZE,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            max_idle_connections: DEFAULT_MAX_IDLE_CONNECTIONS,
            base_url: DUKASCOPY_BASE_URL.to_string(),
        }
    }
}

// ============================================================================
// Client Builder
// ============================================================================

/// Builder for creating configured Dukascopy client instances.
///
/// # Example
///
/// ```
/// use dukascopy_fx::advanced::DukascopyClientBuilder;
///
/// let client = DukascopyClientBuilder::new()
///     .cache_size(500)
///     .timeout_secs(60)
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
pub struct DukascopyClientBuilder {
    config: ClientConfig,
    instrument_provider: Option<OverrideInstrumentProvider>,
}

impl DukascopyClientBuilder {
    /// Creates a new builder with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the cache size (number of hourly data files to cache).
    pub fn cache_size(mut self, size: usize) -> Self {
        self.config.cache_size = size;
        self
    }

    /// Sets the HTTP request timeout in seconds.
    pub fn timeout_secs(mut self, timeout: u64) -> Self {
        self.config.timeout_secs = timeout;
        self
    }

    /// Sets the maximum idle connections per host.
    pub fn max_idle_connections(mut self, connections: usize) -> Self {
        self.config.max_idle_connections = connections;
        self
    }

    /// Sets a custom base URL for the Dukascopy API.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.config.base_url = url.into();
        self
    }

    /// Adds a custom instrument configuration override.
    pub fn with_instrument_config(
        mut self,
        from: &str,
        to: &str,
        config: InstrumentConfig,
    ) -> Self {
        let provider = self
            .instrument_provider
            .get_or_insert_with(OverrideInstrumentProvider::new);
        provider.add_override(from, to, config);
        self
    }

    /// Builds the configured client instance.
    pub fn build(self) -> ConfiguredClient {
        let cache_size = self.config.cache_size.max(1);
        let cache = Arc::new(Mutex::new(LruCache::new(
            NonZeroUsize::new(cache_size).expect("Cache size must be non-zero"),
        )));

        let http_client = Client::builder()
            .pool_max_idle_per_host(self.config.max_idle_connections)
            .tcp_nodelay(true)
            .pool_idle_timeout(None)
            .timeout(StdDuration::from_secs(self.config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        ConfiguredClient {
            config: self.config,
            http_client,
            cache,
            instrument_provider: self.instrument_provider,
        }
    }
}

// ============================================================================
// Configured Client
// ============================================================================

/// A configured Dukascopy client instance with its own cache and settings.
#[derive(Debug)]
pub struct ConfiguredClient {
    config: ClientConfig,
    http_client: Client,
    cache: Arc<Mutex<LruCache<String, Vec<u8>>>>,
    instrument_provider: Option<OverrideInstrumentProvider>,
}

impl ConfiguredClient {
    /// Returns the client configuration.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Returns the instrument configuration for a currency pair.
    pub fn get_instrument_config(&self, from: &str, to: &str) -> InstrumentConfig {
        if let Some(ref provider) = self.instrument_provider {
            provider.get_config(from, to)
        } else {
            crate::core::instrument::resolve_instrument_config(from, to)
        }
    }

    /// Fetches tick data for a currency pair at a specific time.
    pub async fn get_tick_data(
        &self,
        pair: &CurrencyPair,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<u8>, DukascopyError> {
        let url = self.build_url(
            &pair.as_symbol(),
            timestamp.year(),
            timestamp.month(),
            timestamp.day(),
            timestamp.hour(),
        );
        self.fetch_cached(&url).await
    }

    /// Builds a URL for fetching tick data.
    pub fn build_url(
        &self,
        pair_symbol: &str,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
    ) -> String {
        build_tick_url(&self.config.base_url, pair_symbol, year, month, day, hour)
    }

    /// Fetches data from URL with caching.
    async fn fetch_cached(&self, url: &str) -> Result<Vec<u8>, DukascopyError> {
        // Check cache first
        {
            let mut cache_guard = self
                .cache
                .lock()
                .map_err(|e| DukascopyError::CacheError(format!("Cache lock poisoned: {}", e)))?;

            if let Some(data) = cache_guard.get(url) {
                debug!("Cache hit for: {}", url);
                return Ok(data.clone());
            }
        }

        debug!("Cache miss for: {}", url);
        info!("Fetching data from: {}", url);

        // Fetch from network
        let response = self.http_client.get(url).send().await?;

        if !response.status().is_success() {
            warn!("HTTP error {} for: {}", response.status(), url);
            return Err(map_http_error(response.status()));
        }

        let bytes = response.bytes().await?;

        if bytes.is_empty() {
            warn!("Empty response for: {}", url);
            return Err(DukascopyError::DataNotFound);
        }

        // Decompress in blocking task
        let decompressed = tokio::task::spawn_blocking(move || {
            let mut output = Vec::with_capacity(bytes.len() * 4);
            lzma_rs::lzma_decompress(&mut Cursor::new(&bytes), &mut output)?;
            Ok::<_, DukascopyError>(output)
        })
        .await??;

        if decompressed.is_empty() {
            return Err(DukascopyError::DataNotFound);
        }

        debug!("Fetched and decompressed {} bytes", decompressed.len());

        // Cache the result
        {
            let mut cache_guard = self
                .cache
                .lock()
                .map_err(|e| DukascopyError::CacheError(format!("Cache lock poisoned: {}", e)))?;
            cache_guard.put(url.to_string(), decompressed.clone());
        }

        Ok(decompressed)
    }

    /// Clears the cache.
    pub fn clear_cache(&self) -> Result<(), DukascopyError> {
        let mut cache_guard = self
            .cache
            .lock()
            .map_err(|e| DukascopyError::CacheError(format!("Cache lock poisoned: {}", e)))?;
        cache_guard.clear();
        debug!("Cache cleared");
        Ok(())
    }

    /// Returns the current number of cached entries.
    pub fn cache_len(&self) -> Result<usize, DukascopyError> {
        let cache_guard = self
            .cache
            .lock()
            .map_err(|e| DukascopyError::CacheError(format!("Cache lock poisoned: {}", e)))?;
        Ok(cache_guard.len())
    }
}

// ============================================================================
// Static Client API
// ============================================================================

/// Static convenience API using a global default client.
pub struct DukascopyClient;

impl DukascopyClient {
    /// Fetches the exchange rate for a currency pair at a specific timestamp.
    pub async fn get_exchange_rate(
        pair: &CurrencyPair,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        Self::validate_pair(pair)?;

        let effective_timestamp = if is_weekend(timestamp) {
            last_available_tick_time(timestamp)
        } else {
            timestamp
        };

        let url = Self::build_url(
            &pair.as_symbol(),
            effective_timestamp.year(),
            effective_timestamp.month(),
            effective_timestamp.day(),
            effective_timestamp.hour(),
        );

        let data = get_default_client().await.fetch_cached(&url).await?;
        DukascopyParser::validate_decompressed_data(&data)?;

        let target_ms = Self::timestamp_to_ms_from_hour(effective_timestamp);
        let config = pair.instrument_config();
        let tick = Self::find_tick_at_or_before(&data, target_ms, config)?;

        Self::build_exchange_response(pair, effective_timestamp, tick)
    }

    /// Fetches exchange rates over a time range.
    pub async fn get_exchange_rates_range(
        pair: &CurrencyPair,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        interval: Duration,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        Self::validate_pair(pair)?;

        if start >= end {
            return Err(DukascopyError::InvalidRequest(
                "Start time must be before end time".to_string(),
            ));
        }

        let mut results = Vec::new();
        let mut current = start;

        while current <= end {
            match Self::get_exchange_rate(pair, current).await {
                Ok(exchange) => results.push(exchange),
                Err(DukascopyError::DataNotFound) => {}
                Err(e) => return Err(e),
            }
            current += interval;
        }

        Ok(results)
    }

    /// Builds a URL for fetching tick data.
    pub fn build_url(pair_symbol: &str, year: i32, month: u32, day: u32, hour: u32) -> String {
        build_tick_url(DUKASCOPY_BASE_URL, pair_symbol, year, month, day, hour)
    }

    /// Fetches cached data from URL.
    pub async fn get_cached_data(url: &str) -> Result<Vec<u8>, DukascopyError> {
        get_default_client().await.fetch_cached(url).await
    }

    /// Clears the global cache.
    pub async fn clear_cache() -> Result<(), DukascopyError> {
        get_default_client().await.clear_cache()
    }

    /// Gets current cache size.
    pub async fn cache_len() -> Result<usize, DukascopyError> {
        get_default_client().await.cache_len()
    }

    // Private helpers

    fn validate_pair(pair: &CurrencyPair) -> Result<(), DukascopyError> {
        if pair.from().len() != 3 {
            return Err(DukascopyError::InvalidCurrencyCode {
                code: pair.from().to_string(),
                reason: "Currency code must be exactly 3 characters".to_string(),
            });
        }
        if pair.to().len() != 3 {
            return Err(DukascopyError::InvalidCurrencyCode {
                code: pair.to().to_string(),
                reason: "Currency code must be exactly 3 characters".to_string(),
            });
        }
        Ok(())
    }

    fn timestamp_to_ms_from_hour(timestamp: DateTime<Utc>) -> u32 {
        timestamp.minute() * 60_000 + timestamp.second() * 1_000
    }

    fn find_tick_at_or_before(
        data: &[u8],
        target_ms: u32,
        config: InstrumentConfig,
    ) -> Result<ParsedTick, DukascopyError> {
        let mut best_tick: Option<ParsedTick> = None;
        let mut first_tick: Option<ParsedTick> = None;

        for chunk in data.chunks_exact(TICK_SIZE_BYTES) {
            let tick = DukascopyParser::parse_tick_with_config(chunk, config)?;

            if first_tick.is_none() {
                first_tick = Some(tick);
            }

            if tick.ms_from_hour <= target_ms {
                best_tick = Some(tick);
            } else {
                break;
            }
        }

        best_tick.or(first_tick).ok_or(DukascopyError::DataNotFound)
    }

    fn build_exchange_response(
        pair: &CurrencyPair,
        base_timestamp: DateTime<Utc>,
        tick: ParsedTick,
    ) -> Result<CurrencyExchange, DukascopyError> {
        let mid_price = tick.mid_price();

        let rate = Decimal::from_f64(mid_price)
            .ok_or_else(|| DukascopyError::Unknown("Invalid price conversion".to_string()))?;
        let rate =
            rate.round_dp_with_strategy(RATE_DECIMAL_PLACES, RoundingStrategy::MidpointNearestEven);

        let ask = Decimal::from_f64(tick.ask)
            .ok_or_else(|| DukascopyError::Unknown("Invalid ask price conversion".to_string()))?;
        let ask = ask.round_dp_with_strategy(
            RATE_DECIMAL_PLACES + 1,
            RoundingStrategy::MidpointNearestEven,
        );

        let bid = Decimal::from_f64(tick.bid)
            .ok_or_else(|| DukascopyError::Unknown("Invalid bid price conversion".to_string()))?;
        let bid = bid.round_dp_with_strategy(
            RATE_DECIMAL_PLACES + 1,
            RoundingStrategy::MidpointNearestEven,
        );

        let tick_time = base_timestamp
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .ok_or_else(|| DukascopyError::Unknown("Invalid timestamp".to_string()))?
            + Duration::milliseconds(tick.ms_from_hour as i64);

        Ok(CurrencyExchange {
            pair: pair.clone(),
            rate,
            timestamp: tick_time,
            ask,
            bid,
            ask_volume: tick.ask_volume,
            bid_volume: tick.bid_volume,
        })
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn build_tick_url(
    base_url: &str,
    pair_symbol: &str,
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
) -> String {
    format!(
        "{}/{}/{}/{:02}/{:02}/{}h_ticks.bi5",
        base_url,
        pair_symbol,
        year,
        month - 1,
        day,
        hour
    )
}

fn map_http_error(status: reqwest::StatusCode) -> DukascopyError {
    match status {
        reqwest::StatusCode::NOT_FOUND => DukascopyError::DataNotFound,
        reqwest::StatusCode::TOO_MANY_REQUESTS => DukascopyError::RateLimitExceeded,
        reqwest::StatusCode::UNAUTHORIZED => DukascopyError::Unauthorized,
        reqwest::StatusCode::FORBIDDEN => DukascopyError::Forbidden,
        reqwest::StatusCode::BAD_REQUEST => {
            DukascopyError::InvalidRequest("Bad request".to_string())
        }
        status => DukascopyError::HttpError(format!(
            "HTTP {} - {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown")
        )),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_url() {
        let url = DukascopyClient::build_url("EURUSD", 2024, 1, 15, 14);
        assert_eq!(
            url,
            "https://datafeed.dukascopy.com/datafeed/EURUSD/2024/00/15/14h_ticks.bi5"
        );
    }

    #[test]
    fn test_build_url_december() {
        let url = DukascopyClient::build_url("USDJPY", 2024, 12, 31, 23);
        assert_eq!(
            url,
            "https://datafeed.dukascopy.com/datafeed/USDJPY/2024/11/31/23h_ticks.bi5"
        );
    }

    #[test]
    fn test_map_http_error() {
        assert!(matches!(
            map_http_error(reqwest::StatusCode::NOT_FOUND),
            DukascopyError::DataNotFound
        ));
        assert!(matches!(
            map_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS),
            DukascopyError::RateLimitExceeded
        ));
    }

    #[test]
    fn test_default_config() {
        let config = ClientConfig::default();
        assert_eq!(config.cache_size, DEFAULT_CACHE_SIZE);
        assert_eq!(config.timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn test_builder_chaining() {
        let client = DukascopyClientBuilder::new()
            .cache_size(200)
            .timeout_secs(45)
            .build();

        assert_eq!(client.config().cache_size, 200);
        assert_eq!(client.config().timeout_secs, 45);
    }

    #[test]
    fn test_timestamp_to_ms() {
        use chrono::TimeZone;
        let ts = Utc.with_ymd_and_hms(2024, 1, 1, 14, 30, 15).unwrap();
        assert_eq!(
            DukascopyClient::timestamp_to_ms_from_hour(ts),
            30 * 60_000 + 15 * 1_000
        );
    }
}
