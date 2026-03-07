//! HTTP client for fetching tick data from Dukascopy.

use crate::core::instrument::{InstrumentConfig, InstrumentProvider, OverrideInstrumentProvider};
use crate::core::parser::{DukascopyParser, ParsedTick, TICK_SIZE_BYTES};
use crate::error::DukascopyError;
use crate::market::{is_market_open, last_available_tick_time};
use crate::models::{CurrencyExchange, CurrencyPair, RateRequest};

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
#[cfg(feature = "logging")]
use log::{debug, info, warn};
use lru::LruCache;
use reqwest::Client;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use tokio::sync::{OnceCell, Semaphore};

#[cfg(not(feature = "logging"))]
macro_rules! debug {
    ($($arg:tt)*) => {{
        let _ = format_args!($($arg)*);
    }};
}

#[cfg(not(feature = "logging"))]
macro_rules! info {
    ($($arg:tt)*) => {{
        let _ = format_args!($($arg)*);
    }};
}

#[cfg(not(feature = "logging"))]
macro_rules! warn {
    ($($arg:tt)*) => {{
        let _ = format_args!($($arg)*);
    }};
}

// ============================================================================
// Constants
// ============================================================================

/// Default LRU cache size for decompressed tick data
pub const DEFAULT_CACHE_SIZE: usize = 100;

/// Default maximum idle connections per host
pub const DEFAULT_MAX_IDLE_CONNECTIONS: usize = 10;

/// Default HTTP request timeout in seconds
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default retry attempts for transient HTTP/network failures
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// Default base delay for exponential retry backoff in milliseconds
pub const DEFAULT_RETRY_BASE_DELAY_MS: u64 = 200;

/// Default maximum number of in-flight HTTP requests per client
pub const DEFAULT_MAX_IN_FLIGHT_REQUESTS: usize = 8;

/// Maximum number of hours to backtrack when resolving at-or-before tick.
const MAX_AT_OR_BEFORE_BACKTRACK_HOURS: usize = 72;

/// Dukascopy API base URL
pub const DUKASCOPY_BASE_URL: &str = "https://datafeed.dukascopy.com/datafeed";

/// Default quote currency used by global convenience symbol API.
pub const GLOBAL_DEFAULT_QUOTE_CURRENCY: &str = "USD";

/// How symbol-only requests should be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PairResolutionMode {
    /// Symbol-only requests are disabled.
    ExplicitOnly,
    /// Symbol-only requests use configured default quote currency.
    #[default]
    ExplicitOrDefaultQuote,
}

/// How cross-currency conversion should be performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConversionMode {
    /// Use only direct instrument/quote pairs.
    #[default]
    DirectOnly,
    /// Try direct pair, then fallback to synthetic route through bridge currencies.
    DirectThenSynthetic,
}

/// Conversion path type for symbol/quote resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionPathType {
    Direct,
    Synthetic,
}

/// Rich output for symbol/quote resolution.
#[derive(Debug, Clone)]
pub struct ResolvedExchange {
    pub exchange: CurrencyExchange,
    pub path_type: ConversionPathType,
    pub legs: Vec<CurrencyExchange>,
}

fn normalize_code(code: &str) -> String {
    code.trim().to_ascii_uppercase()
}

// Global default client instance
static DEFAULT_CLIENT: OnceCell<ConfiguredClient> = OnceCell::const_new();

/// Gets or initializes the global default client
async fn get_default_client() -> &'static ConfiguredClient {
    DEFAULT_CLIENT
        .get_or_init(|| async {
            DukascopyClientBuilder::new()
                .default_quote_currency(GLOBAL_DEFAULT_QUOTE_CURRENCY)
                .build()
        })
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
    /// Maximum retries for transient failures
    pub max_retries: u32,
    /// Base delay for exponential retry backoff in milliseconds
    pub retry_base_delay_ms: u64,
    /// Maximum number of concurrent in-flight HTTP requests
    pub max_in_flight_requests: usize,
    /// Whether market-hours filtering should be applied (FX-style).
    pub respect_market_hours: bool,
    /// Optional default quote currency used for symbol-only requests.
    pub default_quote_currency: Option<String>,
    /// Symbol resolution policy.
    pub pair_resolution_mode: PairResolutionMode,
    /// Conversion policy for quote resolution.
    pub conversion_mode: ConversionMode,
    /// Bridge currencies used by synthetic conversion.
    pub bridge_currencies: Vec<String>,
    /// Optional alias mapping for instrument codes, e.g. `AAPL -> AAPLUS`.
    pub code_aliases: HashMap<String, String>,
    /// Base URL for the Dukascopy API
    pub base_url: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            cache_size: DEFAULT_CACHE_SIZE,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            max_idle_connections: DEFAULT_MAX_IDLE_CONNECTIONS,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_base_delay_ms: DEFAULT_RETRY_BASE_DELAY_MS,
            max_in_flight_requests: DEFAULT_MAX_IN_FLIGHT_REQUESTS,
            respect_market_hours: true,
            default_quote_currency: None,
            pair_resolution_mode: PairResolutionMode::default(),
            conversion_mode: ConversionMode::default(),
            bridge_currencies: vec!["USD".to_string(), "EUR".to_string()],
            code_aliases: HashMap::new(),
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

    /// Sets the maximum number of retries for transient failures.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.config.max_retries = retries;
        self
    }

    /// Sets the base delay for exponential retry backoff in milliseconds.
    pub fn retry_base_delay_ms(mut self, delay_ms: u64) -> Self {
        self.config.retry_base_delay_ms = delay_ms;
        self
    }

    /// Sets the maximum number of in-flight HTTP requests.
    pub fn max_in_flight_requests(mut self, max_requests: usize) -> Self {
        self.config.max_in_flight_requests = max_requests;
        self
    }

    /// Enables or disables FX market-hours filtering.
    pub fn respect_market_hours(mut self, enabled: bool) -> Self {
        self.config.respect_market_hours = enabled;
        self
    }

    /// Sets default quote currency for symbol-only requests.
    pub fn default_quote_currency(mut self, quote: &str) -> Self {
        self.config.default_quote_currency = Some(normalize_code(quote));
        self
    }

    /// Clears default quote currency.
    pub fn clear_default_quote_currency(mut self) -> Self {
        self.config.default_quote_currency = None;
        self
    }

    /// Sets symbol resolution policy.
    pub fn pair_resolution_mode(mut self, mode: PairResolutionMode) -> Self {
        self.config.pair_resolution_mode = mode;
        self
    }

    /// Sets conversion policy.
    pub fn conversion_mode(mut self, mode: ConversionMode) -> Self {
        self.config.conversion_mode = mode;
        self
    }

    /// Sets bridge currencies used by synthetic conversion fallback.
    pub fn bridge_currencies(mut self, currencies: &[&str]) -> Self {
        let mut bridges = Vec::with_capacity(currencies.len());
        for currency in currencies {
            let normalized = normalize_code(currency);
            if normalized.is_empty() || bridges.contains(&normalized) {
                continue;
            }
            bridges.push(normalized);
        }
        if !bridges.is_empty() {
            self.config.bridge_currencies = bridges;
        }
        self
    }

    /// Adds a code alias mapping used by request resolution, e.g. `AAPL -> AAPLUS`.
    pub fn code_alias(mut self, alias: &str, canonical: &str) -> Self {
        let alias = normalize_code(alias);
        let canonical = normalize_code(canonical);
        if !alias.is_empty() && !canonical.is_empty() && alias != canonical {
            self.config.code_aliases.insert(alias, canonical);
        }
        self
    }

    /// Adds multiple code alias mappings.
    pub fn code_aliases(mut self, aliases: &[(&str, &str)]) -> Self {
        for (alias, canonical) in aliases {
            let alias = normalize_code(alias);
            let canonical = normalize_code(canonical);
            if alias.is_empty() || canonical.is_empty() || alias == canonical {
                continue;
            }
            self.config.code_aliases.insert(alias, canonical);
        }
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

    /// Imports instrument configs and aliases from a catalog.
    pub fn with_instrument_catalog(
        mut self,
        catalog: &crate::core::catalog::InstrumentCatalog,
    ) -> Self {
        let provider = self
            .instrument_provider
            .get_or_insert_with(OverrideInstrumentProvider::new);
        for instrument in &catalog.instruments {
            provider.add_override(
                &instrument.base,
                &instrument.quote,
                InstrumentConfig::new(instrument.price_divisor, instrument.decimal_places),
            );
        }

        for (alias, canonical) in catalog.normalized_code_aliases() {
            self.config.code_aliases.insert(alias, canonical);
        }

        self
    }

    /// Builds the configured client instance.
    pub fn build(self) -> ConfiguredClient {
        let config = self.config;
        let cache_size = config.cache_size.max(1);
        let max_in_flight_requests = config.max_in_flight_requests.max(1);
        let cache_capacity = NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::MIN);
        let cache = Arc::new(Mutex::new(LruCache::new(cache_capacity)));

        let http_client = match Client::builder()
            .pool_max_idle_per_host(config.max_idle_connections)
            .tcp_nodelay(true)
            .pool_idle_timeout(None)
            .timeout(StdDuration::from_secs(config.timeout_secs))
            .build()
        {
            Ok(client) => client,
            Err(err) => {
                warn!(
                    "Failed to create custom HTTP client config (falling back to reqwest::Client::new()): {}",
                    err
                );
                Client::new()
            }
        };

        ConfiguredClient {
            config,
            http_client,
            cache,
            request_limiter: Arc::new(Semaphore::new(max_in_flight_requests)),
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
    request_limiter: Arc<Semaphore>,
    instrument_provider: Option<OverrideInstrumentProvider>,
}

impl ConfiguredClient {
    /// Returns the client configuration.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Returns configured default quote currency if set.
    pub fn default_quote_currency(&self) -> Option<&str> {
        self.config.default_quote_currency.as_deref()
    }

    /// Returns the instrument configuration for a currency pair.
    pub fn get_instrument_config(&self, from: &str, to: &str) -> InstrumentConfig {
        let from = self.resolve_code_alias(from);
        let to = self.resolve_code_alias(to);
        if let Some(ref provider) = self.instrument_provider {
            provider.get_config(&from, &to)
        } else {
            crate::core::instrument::resolve_instrument_config(&from, &to)
        }
    }

    /// Fetches tick data for a currency pair at a specific time.
    pub async fn get_tick_data(
        &self,
        pair: &CurrencyPair,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<u8>, DukascopyError> {
        let resolved_pair = self.resolve_pair_alias(pair)?;
        let url = self.build_url(
            &resolved_pair.as_symbol(),
            timestamp.year(),
            timestamp.month(),
            timestamp.day(),
            timestamp.hour(),
        );
        self.fetch_cached(&url).await
    }

    /// Fetches the exchange rate for a currency pair at a specific timestamp.
    pub async fn get_exchange_rate(
        &self,
        pair: &CurrencyPair,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        let resolved_pair = self.resolve_pair_alias(pair)?;
        DukascopyClient::validate_pair(&resolved_pair)?;

        let effective_timestamp = if self.config.respect_market_hours && !is_market_open(timestamp)
        {
            last_available_tick_time(timestamp)
        } else {
            timestamp
        };

        let config = self.get_instrument_config(resolved_pair.from(), resolved_pair.to());
        let mut query_timestamp = effective_timestamp;
        let mut fallback_attempts = 0usize;

        loop {
            let hour_start = DukascopyClient::hour_start(query_timestamp)?;
            let url = self.build_url(
                &resolved_pair.as_symbol(),
                hour_start.year(),
                hour_start.month(),
                hour_start.day(),
                hour_start.hour(),
            );

            let data = match self.fetch_cached(&url).await {
                Ok(data) => data,
                Err(DukascopyError::DataNotFound) if fallback_attempts > 0 => {
                    if fallback_attempts >= MAX_AT_OR_BEFORE_BACKTRACK_HOURS {
                        return Err(DukascopyError::DataNotFound);
                    }
                    query_timestamp = hour_start
                        .checked_sub_signed(Duration::milliseconds(1))
                        .ok_or(DukascopyError::DataNotFound)?;
                    fallback_attempts += 1;
                    continue;
                }
                Err(err) => return Err(err),
            };
            DukascopyParser::validate_decompressed_data(&data)?;

            let target_ms = DukascopyClient::timestamp_to_ms_from_hour(query_timestamp);
            match DukascopyClient::find_tick_at_or_before(&data, target_ms, config) {
                Ok(tick) => {
                    return DukascopyClient::build_exchange_response(
                        pair, hour_start, tick, config,
                    );
                }
                Err(DukascopyError::DataNotFound) => {
                    if fallback_attempts >= MAX_AT_OR_BEFORE_BACKTRACK_HOURS {
                        return Err(DukascopyError::DataNotFound);
                    }
                    query_timestamp = hour_start
                        .checked_sub_signed(Duration::milliseconds(1))
                        .ok_or(DukascopyError::DataNotFound)?;
                    fallback_attempts += 1;
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// Fetches exchange rate for unified request type (pair or symbol).
    pub async fn get_exchange_rate_for_request(
        &self,
        request: &RateRequest,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        match request {
            RateRequest::Pair(pair) => self.get_exchange_rate(pair, timestamp).await,
            RateRequest::Symbol(symbol) => {
                self.get_exchange_rate_for_symbol(symbol, timestamp).await
            }
        }
    }

    /// Fetches exchange rate for a symbol using configured default quote currency.
    pub async fn get_exchange_rate_for_symbol(
        &self,
        symbol: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        self.get_exchange_rate_for_symbol_with_path(symbol, timestamp)
            .await
            .map(|resolved| resolved.exchange)
    }

    /// Fetches exchange rate for a symbol and returns conversion path metadata.
    pub async fn get_exchange_rate_for_symbol_with_path(
        &self,
        symbol: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<ResolvedExchange, DukascopyError> {
        if self.config.pair_resolution_mode == PairResolutionMode::ExplicitOnly {
            return Err(DukascopyError::PairResolutionDisabled);
        }

        let quote = self
            .config
            .default_quote_currency
            .as_deref()
            .ok_or(DukascopyError::MissingDefaultQuoteCurrency)?;

        self.get_exchange_rate_in_quote_with_path(symbol, quote, timestamp)
            .await
    }

    /// Fetches exchange rate for a symbol in target quote currency.
    pub async fn get_exchange_rate_in_quote(
        &self,
        symbol: &str,
        quote: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        self.get_exchange_rate_in_quote_with_path(symbol, quote, timestamp)
            .await
            .map(|resolved| resolved.exchange)
    }

    /// Fetches exchange rate for a symbol in target quote currency with path metadata.
    pub async fn get_exchange_rate_in_quote_with_path(
        &self,
        symbol: &str,
        quote: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<ResolvedExchange, DukascopyError> {
        let symbol = self.resolve_code_alias(symbol);
        let quote = self.resolve_code_alias(quote);
        if symbol == quote {
            let pair = CurrencyPair::try_new(symbol.clone(), quote.clone())?;
            let effective_timestamp =
                if self.config.respect_market_hours && !is_market_open(timestamp) {
                    last_available_tick_time(timestamp)
                } else {
                    timestamp
                };

            return Ok(ResolvedExchange {
                exchange: CurrencyExchange {
                    pair,
                    rate: Decimal::ONE,
                    timestamp: effective_timestamp,
                    ask: Decimal::ONE,
                    bid: Decimal::ONE,
                    ask_volume: 0.0,
                    bid_volume: 0.0,
                },
                path_type: ConversionPathType::Direct,
                legs: Vec::new(),
            });
        }

        if let Some(exchange) = self
            .get_exchange_rate_direct_or_inverse(&symbol, &quote, timestamp)
            .await?
        {
            return Ok(ResolvedExchange {
                exchange: exchange.clone(),
                path_type: ConversionPathType::Direct,
                legs: vec![exchange],
            });
        }

        if self.config.conversion_mode == ConversionMode::DirectOnly {
            return Err(DukascopyError::NoConversionRoute { symbol, quote });
        }

        for bridge in &self.config.bridge_currencies {
            if bridge == &quote || bridge == &symbol {
                continue;
            }

            let first_leg = match self
                .get_exchange_rate_direct_or_inverse(&symbol, bridge, timestamp)
                .await?
            {
                Some(rate) => rate,
                None => continue,
            };

            let second_leg = match self
                .get_exchange_rate_direct_or_inverse(bridge, &quote, timestamp)
                .await?
            {
                Some(rate) => rate,
                None => continue,
            };

            let exchange = DukascopyClient::build_synthetic_exchange(
                &symbol,
                &quote,
                &first_leg,
                &second_leg,
            )?;

            return Ok(ResolvedExchange {
                exchange,
                path_type: ConversionPathType::Synthetic,
                legs: vec![first_leg, second_leg],
            });
        }

        Err(DukascopyError::NoConversionRoute { symbol, quote })
    }

    async fn get_exchange_rate_direct_or_inverse(
        &self,
        from: &str,
        to: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<CurrencyExchange>, DukascopyError> {
        let direct_pair = CurrencyPair::try_new(from.to_string(), to.to_string())?;
        match self.get_exchange_rate(&direct_pair, timestamp).await {
            Ok(exchange) => return Ok(Some(exchange)),
            Err(DukascopyError::DataNotFound) => {}
            Err(err) => return Err(err),
        }

        let inverse_pair = CurrencyPair::try_new(to.to_string(), from.to_string())?;
        match self.get_exchange_rate(&inverse_pair, timestamp).await {
            Ok(exchange) => Ok(Some(DukascopyClient::invert_exchange(&exchange)?)),
            Err(DukascopyError::DataNotFound) => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn resolve_code_alias(&self, code: &str) -> String {
        let mut current = normalize_code(code);
        let mut visited = HashSet::new();

        while let Some(next) = self.config.code_aliases.get(&current) {
            if !visited.insert(current.clone()) {
                break;
            }
            if next == &current {
                break;
            }
            current = next.clone();
        }

        current
    }

    fn resolve_pair_alias(&self, pair: &CurrencyPair) -> Result<CurrencyPair, DukascopyError> {
        let from = self.resolve_code_alias(pair.from());
        let to = self.resolve_code_alias(pair.to());
        CurrencyPair::try_new(from, to)
    }

    /// Fetches exchange rates over a time range.
    pub async fn get_exchange_rates_range(
        &self,
        pair: &CurrencyPair,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        interval: Duration,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        let resolved_pair = self.resolve_pair_alias(pair)?;
        DukascopyClient::validate_pair(&resolved_pair)?;

        if start >= end {
            return Err(DukascopyError::InvalidRequest(
                "Start time must be before end time".to_string(),
            ));
        }

        if interval <= Duration::zero() {
            return Err(DukascopyError::InvalidRequest(
                "Interval must be a positive duration".to_string(),
            ));
        }

        let mut results: Vec<CurrencyExchange> = Vec::new();
        let mut hour_cache: Option<(DateTime<Utc>, Vec<ParsedTick>)> = None;
        let pair_symbol = resolved_pair.as_symbol();
        let config = self.get_instrument_config(resolved_pair.from(), resolved_pair.to());
        let mut current = start;

        while current <= end {
            if self.config.respect_market_hours && !is_market_open(current) {
                current += interval;
                continue;
            }

            let hour_start = DukascopyClient::hour_start(current)?;
            let cache_miss = hour_cache
                .as_ref()
                .map(|(cached_hour, _)| *cached_hour != hour_start)
                .unwrap_or(true);

            if cache_miss {
                let url = self.build_url(
                    &pair_symbol,
                    hour_start.year(),
                    hour_start.month(),
                    hour_start.day(),
                    hour_start.hour(),
                );

                match self.fetch_cached(&url).await {
                    Ok(data) => {
                        DukascopyParser::validate_decompressed_data(&data)?;
                        let mut parsed_ticks = Vec::with_capacity(data.len() / TICK_SIZE_BYTES);
                        for tick in DukascopyParser::iter_ticks(&data, config) {
                            parsed_ticks.push(tick?);
                        }
                        hour_cache = Some((hour_start, parsed_ticks));
                    }
                    Err(DukascopyError::DataNotFound) => {
                        hour_cache = Some((hour_start, Vec::new()));
                        current += interval;
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }

            let target_ms = DukascopyClient::timestamp_to_ms_from_hour(current);
            let mut exchange = match hour_cache.as_ref() {
                Some((_, ticks)) => {
                    if let Some(tick) =
                        DukascopyClient::find_tick_at_or_before_parsed(ticks, target_ms)
                    {
                        Some(DukascopyClient::build_exchange_response(
                            pair, current, tick, config,
                        )?)
                    } else {
                        None
                    }
                }
                None => None,
            };

            if exchange.is_none() {
                let fallback_ts = current
                    .checked_sub_signed(Duration::milliseconds(1))
                    .ok_or(DukascopyError::DataNotFound)?;
                exchange = match self.get_exchange_rate(pair, fallback_ts).await {
                    Ok(value) => Some(value),
                    Err(DukascopyError::DataNotFound) => None,
                    Err(err) => return Err(err),
                };
            }

            if let Some(exchange) = exchange {
                let duplicate_ts = results
                    .last()
                    .map(|last| last.timestamp == exchange.timestamp)
                    .unwrap_or(false);
                if !duplicate_ts {
                    results.push(exchange);
                }
            }

            current += interval;
        }

        Ok(results)
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

        let _request_permit = self.request_limiter.acquire().await.map_err(|_| {
            DukascopyError::Unknown("Request limiter was closed unexpectedly".to_string())
        })?;

        let mut attempt = 0;
        let bytes = loop {
            match self.http_client.get(url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if !status.is_success() {
                        let error = map_http_error(status);
                        if error.is_retryable() && attempt < self.config.max_retries {
                            let delay_ms = retry_delay_ms(self.config.retry_base_delay_ms, attempt);
                            warn!(
                                "Request failed with {} for: {} (attempt {}/{}, retrying in {} ms)",
                                status,
                                url,
                                attempt + 1,
                                self.config.max_retries + 1,
                                delay_ms
                            );
                            tokio::time::sleep(StdDuration::from_millis(delay_ms)).await;
                            attempt += 1;
                            continue;
                        }

                        warn!("HTTP error {} for: {}", status, url);
                        return Err(error);
                    }

                    match response.bytes().await {
                        Ok(bytes) => break bytes,
                        Err(err) => {
                            let error = self.map_reqwest_error(err);
                            if error.is_retryable() && attempt < self.config.max_retries {
                                let delay_ms =
                                    retry_delay_ms(self.config.retry_base_delay_ms, attempt);
                                warn!(
                                    "Failed to read response body for: {} (attempt {}/{}, retrying in {} ms): {}",
                                    url,
                                    attempt + 1,
                                    self.config.max_retries + 1,
                                    delay_ms,
                                    error
                                );
                                tokio::time::sleep(StdDuration::from_millis(delay_ms)).await;
                                attempt += 1;
                                continue;
                            }
                            return Err(error);
                        }
                    }
                }
                Err(err) => {
                    let error = self.map_reqwest_error(err);
                    if error.is_retryable() && attempt < self.config.max_retries {
                        let delay_ms = retry_delay_ms(self.config.retry_base_delay_ms, attempt);
                        warn!(
                            "Network request failed for: {} (attempt {}/{}, retrying in {} ms): {}",
                            url,
                            attempt + 1,
                            self.config.max_retries + 1,
                            delay_ms,
                            error
                        );
                        tokio::time::sleep(StdDuration::from_millis(delay_ms)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(error);
                }
            }
        };

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

    fn map_reqwest_error(&self, err: reqwest::Error) -> DukascopyError {
        if err.is_timeout() {
            DukascopyError::Timeout(self.config.timeout_secs)
        } else if err.is_connect() {
            DukascopyError::HttpError(format!("Connection failed: {}", err))
        } else {
            DukascopyError::HttpError(err.to_string())
        }
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
        get_default_client()
            .await
            .get_exchange_rate(pair, timestamp)
            .await
    }

    /// Fetches exchange rate for unified request type (pair or symbol).
    pub async fn get_exchange_rate_for_request(
        request: &RateRequest,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        get_default_client()
            .await
            .get_exchange_rate_for_request(request, timestamp)
            .await
    }

    /// Fetches exchange rates over a time range.
    pub async fn get_exchange_rates_range(
        pair: &CurrencyPair,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        interval: Duration,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        get_default_client()
            .await
            .get_exchange_rates_range(pair, start, end, interval)
            .await
    }

    /// Fetches exchange rate for a symbol using global client's default quote currency.
    pub async fn get_exchange_rate_for_symbol(
        symbol: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        get_default_client()
            .await
            .get_exchange_rate_for_symbol(symbol, timestamp)
            .await
    }

    /// Fetches exchange rate for a symbol in target quote currency.
    pub async fn get_exchange_rate_in_quote(
        symbol: &str,
        quote: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        get_default_client()
            .await
            .get_exchange_rate_in_quote(symbol, quote, timestamp)
            .await
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
        if !DukascopyClient::is_valid_instrument_code(pair.from()) {
            return Err(DukascopyError::InvalidCurrencyCode {
                code: pair.from().to_string(),
                reason: "Instrument code must be 2-12 ASCII alphanumeric characters".to_string(),
            });
        }
        if !DukascopyClient::is_valid_instrument_code(pair.to()) {
            return Err(DukascopyError::InvalidCurrencyCode {
                code: pair.to().to_string(),
                reason: "Instrument code must be 2-12 ASCII alphanumeric characters".to_string(),
            });
        }
        Ok(())
    }

    fn is_valid_instrument_code(code: &str) -> bool {
        let len = code.len();
        (2..=12).contains(&len) && code.chars().all(|ch| ch.is_ascii_alphanumeric())
    }

    fn hour_start(timestamp: DateTime<Utc>) -> Result<DateTime<Utc>, DukascopyError> {
        timestamp
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .ok_or_else(|| DukascopyError::Unknown("Invalid timestamp".to_string()))
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

        for chunk in data.chunks_exact(TICK_SIZE_BYTES) {
            let tick = DukascopyParser::parse_tick_with_config(chunk, config)?;

            if tick.ms_from_hour <= target_ms {
                best_tick = Some(tick);
            } else {
                break;
            }
        }

        best_tick.ok_or(DukascopyError::DataNotFound)
    }

    fn find_tick_at_or_before_parsed(ticks: &[ParsedTick], target_ms: u32) -> Option<ParsedTick> {
        if ticks.is_empty() {
            return None;
        }

        match ticks.binary_search_by_key(&target_ms, |tick| tick.ms_from_hour) {
            Ok(index) => Some(ticks[index]),
            Err(0) => None,
            Err(index) => Some(ticks[index - 1]),
        }
    }

    fn build_exchange_response(
        pair: &CurrencyPair,
        base_timestamp: DateTime<Utc>,
        tick: ParsedTick,
        config: InstrumentConfig,
    ) -> Result<CurrencyExchange, DukascopyError> {
        let decimal_places = config.decimal_places;
        let mid_price = tick.mid_price();

        let rate = Decimal::from_f64(mid_price)
            .ok_or_else(|| DukascopyError::Unknown("Invalid price conversion".to_string()))?;
        let rate =
            rate.round_dp_with_strategy(decimal_places, RoundingStrategy::MidpointNearestEven);

        let ask = Decimal::from_f64(tick.ask)
            .ok_or_else(|| DukascopyError::Unknown("Invalid ask price conversion".to_string()))?;
        let ask = ask.round_dp_with_strategy(decimal_places, RoundingStrategy::MidpointNearestEven);

        let bid = Decimal::from_f64(tick.bid)
            .ok_or_else(|| DukascopyError::Unknown("Invalid bid price conversion".to_string()))?;
        let bid = bid.round_dp_with_strategy(decimal_places, RoundingStrategy::MidpointNearestEven);

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

    fn build_synthetic_exchange(
        symbol: &str,
        quote: &str,
        first_leg: &CurrencyExchange,
        second_leg: &CurrencyExchange,
    ) -> Result<CurrencyExchange, DukascopyError> {
        let pair = CurrencyPair::try_new(symbol.to_string(), quote.to_string())?;
        let rate = first_leg.rate * second_leg.rate;
        let bid = (first_leg.bid * second_leg.bid).min(first_leg.ask * second_leg.ask);
        let ask = (first_leg.ask * second_leg.ask).max(first_leg.bid * second_leg.bid);
        let timestamp = first_leg.timestamp.min(second_leg.timestamp);

        Ok(CurrencyExchange {
            pair,
            rate,
            timestamp,
            ask,
            bid,
            ask_volume: first_leg.ask_volume.min(second_leg.ask_volume),
            bid_volume: first_leg.bid_volume.min(second_leg.bid_volume),
        })
    }

    fn invert_exchange(exchange: &CurrencyExchange) -> Result<CurrencyExchange, DukascopyError> {
        if exchange.rate.is_zero() || exchange.ask.is_zero() || exchange.bid.is_zero() {
            return Err(DukascopyError::InvalidTickData);
        }

        Ok(CurrencyExchange {
            pair: exchange.pair.inverse(),
            rate: Decimal::ONE / exchange.rate,
            timestamp: exchange.timestamp,
            ask: Decimal::ONE / exchange.bid,
            bid: Decimal::ONE / exchange.ask,
            ask_volume: exchange.bid_volume,
            bid_volume: exchange.ask_volume,
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
    let month = month.clamp(1, 12);
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

fn retry_delay_ms(base_delay_ms: u64, attempt: u32) -> u64 {
    let backoff_factor = 2u64.saturating_pow(attempt.min(16));
    base_delay_ms.saturating_mul(backoff_factor).max(1)
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
    fn test_build_url_clamps_invalid_months() {
        let below_range = DukascopyClient::build_url("EURUSD", 2024, 0, 15, 14);
        assert_eq!(
            below_range,
            "https://datafeed.dukascopy.com/datafeed/EURUSD/2024/00/15/14h_ticks.bi5"
        );

        let above_range = DukascopyClient::build_url("EURUSD", 2024, 13, 15, 14);
        assert_eq!(
            above_range,
            "https://datafeed.dukascopy.com/datafeed/EURUSD/2024/11/15/14h_ticks.bi5"
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
        assert_eq!(config.max_retries, DEFAULT_MAX_RETRIES);
        assert_eq!(config.retry_base_delay_ms, DEFAULT_RETRY_BASE_DELAY_MS);
        assert_eq!(
            config.max_in_flight_requests,
            DEFAULT_MAX_IN_FLIGHT_REQUESTS
        );
        assert!(config.respect_market_hours);
        assert_eq!(
            config.pair_resolution_mode,
            PairResolutionMode::ExplicitOrDefaultQuote
        );
        assert_eq!(config.conversion_mode, ConversionMode::DirectOnly);
        assert_eq!(
            config.bridge_currencies,
            vec!["USD".to_string(), "EUR".to_string()]
        );
        assert!(config.code_aliases.is_empty());
    }

    #[tokio::test]
    async fn test_global_default_client_quote_currency() {
        let client = get_default_client().await;
        assert_eq!(
            client.default_quote_currency(),
            Some(GLOBAL_DEFAULT_QUOTE_CURRENCY)
        );
    }

    #[test]
    fn test_builder_chaining() {
        let client = DukascopyClientBuilder::new()
            .cache_size(200)
            .timeout_secs(45)
            .max_retries(5)
            .retry_base_delay_ms(150)
            .max_in_flight_requests(4)
            .respect_market_hours(false)
            .default_quote_currency("pln")
            .pair_resolution_mode(PairResolutionMode::ExplicitOrDefaultQuote)
            .conversion_mode(ConversionMode::DirectThenSynthetic)
            .bridge_currencies(&["usd", "eur", "usd"])
            .code_alias("aapl", "aaplus")
            .build();

        assert_eq!(client.config().cache_size, 200);
        assert_eq!(client.config().timeout_secs, 45);
        assert_eq!(client.config().max_retries, 5);
        assert_eq!(client.config().retry_base_delay_ms, 150);
        assert_eq!(client.config().max_in_flight_requests, 4);
        assert!(!client.config().respect_market_hours);
        assert_eq!(client.default_quote_currency(), Some("PLN"));
        assert_eq!(
            client.config().pair_resolution_mode,
            PairResolutionMode::ExplicitOrDefaultQuote
        );
        assert_eq!(
            client.config().conversion_mode,
            ConversionMode::DirectThenSynthetic
        );
        assert_eq!(
            client.config().bridge_currencies,
            vec!["USD".to_string(), "EUR".to_string()]
        );
        assert_eq!(
            client.config().code_aliases.get("AAPL"),
            Some(&"AAPLUS".to_string())
        );
    }

    #[test]
    fn test_code_alias_resolves_pair() {
        let client = DukascopyClientBuilder::new()
            .code_alias("aapl", "AAPLUS")
            .build();
        let requested = CurrencyPair::new("AAPL", "USD");
        let resolved = client.resolve_pair_alias(&requested).unwrap();
        assert_eq!(resolved.as_symbol(), "AAPLUSUSD");
    }

    #[test]
    fn test_code_alias_chain_resolves_pair() {
        let client = DukascopyClientBuilder::new()
            .code_alias("sp500", "us500")
            .code_alias("us500", "USA500IDX")
            .build();
        let requested = CurrencyPair::new("SP500", "USD");
        let resolved = client.resolve_pair_alias(&requested).unwrap();
        assert_eq!(resolved.as_symbol(), "USA500IDXUSD");
    }

    #[test]
    fn test_with_instrument_catalog_imports_aliases_and_configs() {
        let catalog_json = r#"
        {
          "instruments": [
            {
              "symbol": "AAPLUSUSD",
              "base": "AAPLUS",
              "quote": "USD",
              "asset_class": "equity",
              "price_divisor": 1000.0,
              "decimal_places": 2,
              "active": true
            }
          ],
          "code_aliases": {
            "AAPL": "AAPLUS"
          }
        }
        "#;
        let catalog = crate::core::catalog::InstrumentCatalog::from_json_str(catalog_json).unwrap();
        let client = DukascopyClientBuilder::new()
            .with_instrument_catalog(&catalog)
            .build();

        assert_eq!(
            client.config().code_aliases.get("AAPL"),
            Some(&"AAPLUS".to_string())
        );
        let config = client.get_instrument_config("AAPL", "USD");
        assert_eq!(config.price_divisor, 1000.0);
        assert_eq!(config.decimal_places, 2);
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

    #[test]
    fn test_retry_delay_ms() {
        assert_eq!(retry_delay_ms(100, 0), 100);
        assert_eq!(retry_delay_ms(100, 1), 200);
        assert_eq!(retry_delay_ms(100, 2), 400);
    }

    #[test]
    fn test_validate_pair_accepts_non_three_char_codes() {
        let pair = CurrencyPair::new("DE40", "USD");
        assert!(DukascopyClient::validate_pair(&pair).is_ok());
    }

    #[test]
    fn test_find_tick_at_or_before_parsed() {
        let ticks = vec![
            ParsedTick {
                ms_from_hour: 100,
                ask: 1.1010,
                bid: 1.1000,
                ask_volume: 1.0,
                bid_volume: 1.0,
            },
            ParsedTick {
                ms_from_hour: 1_000,
                ask: 1.1020,
                bid: 1.1010,
                ask_volume: 1.0,
                bid_volume: 1.0,
            },
        ];

        let first = DukascopyClient::find_tick_at_or_before_parsed(&ticks, 50);
        assert!(first.is_none());

        let second = DukascopyClient::find_tick_at_or_before_parsed(&ticks, 1_000).unwrap();
        assert_eq!(second.ms_from_hour, 1_000);

        let last = DukascopyClient::find_tick_at_or_before_parsed(&ticks, 3_000).unwrap();
        assert_eq!(last.ms_from_hour, 1_000);
    }

    #[test]
    fn test_find_tick_at_or_before_rejects_lookahead() {
        let mut data = Vec::new();
        data.extend_from_slice(&100u32.to_be_bytes()); // ms
        data.extend_from_slice(&110_100u32.to_be_bytes()); // ask raw
        data.extend_from_slice(&110_000u32.to_be_bytes()); // bid raw
        data.extend_from_slice(&1.0f32.to_be_bytes()); // ask volume
        data.extend_from_slice(&1.0f32.to_be_bytes()); // bid volume

        let result = DukascopyClient::find_tick_at_or_before(&data, 50, InstrumentConfig::STANDARD);
        assert!(matches!(result, Err(DukascopyError::DataNotFound)));
    }

    #[test]
    fn test_build_synthetic_exchange() {
        use chrono::TimeZone;
        use rust_decimal::Decimal;

        let leg1 = CurrencyExchange {
            pair: CurrencyPair::new("AAPL", "USD"),
            rate: Decimal::new(150, 0),
            timestamp: Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap(),
            ask: Decimal::new(151, 0),
            bid: Decimal::new(149, 0),
            ask_volume: 10.0,
            bid_volume: 8.0,
        };
        let leg2 = CurrencyExchange {
            pair: CurrencyPair::new("USD", "PLN"),
            rate: Decimal::new(4, 0),
            timestamp: Utc.with_ymd_and_hms(2025, 1, 3, 14, 44, 0).unwrap(),
            ask: Decimal::new(41, 1),
            bid: Decimal::new(39, 1),
            ask_volume: 7.0,
            bid_volume: 6.0,
        };

        let synthetic =
            DukascopyClient::build_synthetic_exchange("AAPL", "PLN", &leg1, &leg2).unwrap();

        assert_eq!(synthetic.pair.as_symbol(), "AAPLPLN");
        assert!(synthetic.rate > Decimal::ZERO);
        assert!(synthetic.bid <= synthetic.ask);
        assert_eq!(synthetic.timestamp, leg2.timestamp);
    }

    #[test]
    fn test_invert_exchange() {
        use chrono::TimeZone;
        use rust_decimal::Decimal;

        let original = CurrencyExchange {
            pair: CurrencyPair::new("EUR", "USD"),
            rate: Decimal::new(12, 1), // 1.2
            timestamp: Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap(),
            ask: Decimal::new(121, 2), // 1.21
            bid: Decimal::new(119, 2), // 1.19
            ask_volume: 10.0,
            bid_volume: 7.0,
        };

        let inverted = DukascopyClient::invert_exchange(&original).unwrap();
        assert_eq!(inverted.pair.as_symbol(), "USDEUR");
        assert_eq!(inverted.rate, Decimal::ONE / original.rate);
        assert_eq!(inverted.ask, Decimal::ONE / original.bid);
        assert_eq!(inverted.bid, Decimal::ONE / original.ask);
        assert!(inverted.bid <= inverted.ask);
        assert_eq!(inverted.ask_volume, original.bid_volume);
        assert_eq!(inverted.bid_volume, original.ask_volume);
    }

    #[tokio::test]
    async fn test_get_exchange_rate_for_symbol_requires_default_quote() {
        let client = DukascopyClientBuilder::new().build();
        let ts = Utc::now();
        let result = client.get_exchange_rate_for_symbol("AAPL", ts).await;
        assert!(matches!(
            result,
            Err(DukascopyError::MissingDefaultQuoteCurrency)
        ));
    }

    #[tokio::test]
    async fn test_get_exchange_rate_for_symbol_respects_resolution_mode() {
        let client = DukascopyClientBuilder::new()
            .default_quote_currency("USD")
            .pair_resolution_mode(PairResolutionMode::ExplicitOnly)
            .build();
        let ts = Utc::now();
        let result = client.get_exchange_rate_for_symbol("AAPL", ts).await;
        assert!(matches!(
            result,
            Err(DukascopyError::PairResolutionDisabled)
        ));
    }

    #[tokio::test]
    async fn test_get_exchange_rate_for_request_symbol_requires_default_quote() {
        let client = DukascopyClientBuilder::new().build();
        let ts = Utc::now();
        let request = RateRequest::symbol("AAPL").unwrap();
        let result = client.get_exchange_rate_for_request(&request, ts).await;
        assert!(matches!(
            result,
            Err(DukascopyError::MissingDefaultQuoteCurrency)
        ));
    }

    #[tokio::test]
    async fn test_get_exchange_rate_for_request_symbol_respects_resolution_mode() {
        let client = DukascopyClientBuilder::new()
            .default_quote_currency("USD")
            .pair_resolution_mode(PairResolutionMode::ExplicitOnly)
            .build();
        let ts = Utc::now();
        let request = RateRequest::symbol("AAPL").unwrap();
        let result = client.get_exchange_rate_for_request(&request, ts).await;
        assert!(matches!(
            result,
            Err(DukascopyError::PairResolutionDisabled)
        ));
    }

    #[tokio::test]
    async fn test_get_exchange_rate_for_request_pair_validates_before_network() {
        let client = DukascopyClientBuilder::new().build();
        let ts = Utc::now();
        let invalid_pair = CurrencyPair::new("BAD$", "USD");
        let request = RateRequest::Pair(invalid_pair);
        let result = client.get_exchange_rate_for_request(&request, ts).await;

        assert!(matches!(
            result,
            Err(DukascopyError::InvalidCurrencyCode { code, .. }) if code == "BAD$"
        ));
    }

    #[tokio::test]
    async fn test_get_exchange_rate_in_quote_same_symbol_returns_identity() {
        let client = DukascopyClientBuilder::new()
            .conversion_mode(ConversionMode::DirectOnly)
            .build();
        let ts = Utc::now();
        let exchange = client
            .get_exchange_rate_in_quote("USD", "USD", ts)
            .await
            .unwrap();

        assert_eq!(exchange.pair.as_symbol(), "USDUSD");
        assert_eq!(exchange.rate, Decimal::ONE);
        assert_eq!(exchange.ask, Decimal::ONE);
        assert_eq!(exchange.bid, Decimal::ONE);
    }
}
