//! # dukascopy-fx
//!
//! A production-ready Rust library for fetching historical forex exchange rates,
//! inspired by Python's yfinance library.
//!
//! ## Quick Start
//!
//! ```no_run
//! use dukascopy_fx::{Ticker, datetime};
//!
//! # async fn example() -> dukascopy_fx::Result<()> {
//! // Create a ticker and get data - yfinance style!
//! let ticker = Ticker::new("EUR", "USD");
//!
//! // Get recent rate
//! let rate = ticker.rate().await?;
//! println!("EUR/USD: {}", rate.rate);
//!
//! // Get last week of data
//! let history = ticker.history("1w").await?;
//! for r in history {
//!     println!("{}: {}", r.timestamp, r.rate);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Features
//!
//! - **yfinance-style API**: Familiar `Ticker` object with `history()` method
//! - **Period strings**: Use `"1d"`, `"1w"`, `"1mo"`, `"1y"` for easy time ranges
//! - **Built-in time utilities**: No need to add chrono separately
//! - **Type-safe**: Strong types for currency pairs, rates, and errors
//! - **Automatic handling**: JPY pairs, metals, weekends - all transparent
//!
//! ## Usage Patterns
//!
//! ### Ticker API (Recommended)
//! ```no_run
//! use dukascopy_fx::{Ticker, datetime};
//!
//! # async fn example() -> dukascopy_fx::Result<()> {
//! let eur_usd = Ticker::new("EUR", "USD");
//! let gold = Ticker::xau_usd();
//!
//! // Get rate at specific time
//! let rate = eur_usd.rate_at(datetime!(2024-01-15 14:30 UTC)).await?;
//!
//! // Get historical data with period strings
//! let weekly = eur_usd.history("1w").await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Batch Download
//! ```no_run
//! use dukascopy_fx::{Ticker, download};
//!
//! # async fn example() -> dukascopy_fx::Result<()> {
//! let tickers = vec![
//!     Ticker::eur_usd(),
//!     Ticker::gbp_usd(),
//!     Ticker::usd_jpy(),
//! ];
//!
//! let data = download(&tickers, "1w").await?;
//! for (ticker, rates) in data {
//!     println!("{}: {} records", ticker.symbol(), rates.len());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Simple Function API
//! ```no_run
//! use dukascopy_fx::{get_rate, datetime};
//!
//! # async fn example() -> dukascopy_fx::Result<()> {
//! let rate = get_rate("EUR", "USD", datetime!(2024-01-15 14:30 UTC)).await?;
//! println!("Rate: {}", rate.rate);
//! # Ok(())
//! # }
//! ```

// ============================================================================
// Internal modules
// ============================================================================

mod api;
pub(crate) mod core;

// ============================================================================
// Public modules
// ============================================================================

pub mod error;
pub mod macros;
pub mod market;
pub mod models;
pub mod time;

// ============================================================================
// Core exports
// ============================================================================

pub use api::{download, download_range, Ticker};
pub use error::DukascopyError;
pub use models::{CurrencyExchange, CurrencyPair};

/// Convenient alias for [`DukascopyError`]
pub type Error = DukascopyError;

/// Convenient Result type for this crate
pub type Result<T> = std::result::Result<T, Error>;

// ============================================================================
// Simple Function API
// ============================================================================

use chrono::{DateTime, Duration, Utc};

/// Fetches the exchange rate for a currency pair at a specific timestamp.
#[inline]
pub async fn get_rate(from: &str, to: &str, timestamp: DateTime<Utc>) -> Result<CurrencyExchange> {
    let pair = CurrencyPair::new(from, to);
    core::client::DukascopyClient::get_exchange_rate(&pair, timestamp).await
}

/// Fetches the exchange rate using a [`CurrencyPair`].
#[inline]
pub async fn get_rate_for_pair(
    pair: &CurrencyPair,
    timestamp: DateTime<Utc>,
) -> Result<CurrencyExchange> {
    core::client::DukascopyClient::get_exchange_rate(pair, timestamp).await
}

/// Fetches exchange rates over a time range.
#[inline]
pub async fn get_rates_range(
    from: &str,
    to: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    interval: Duration,
) -> Result<Vec<CurrencyExchange>> {
    let pair = CurrencyPair::new(from, to);
    core::client::DukascopyClient::get_exchange_rates_range(&pair, start, end, interval).await
}

/// Fetches exchange rates over a time range using a [`CurrencyPair`].
#[inline]
pub async fn get_rates_range_for_pair(
    pair: &CurrencyPair,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    interval: Duration,
) -> Result<Vec<CurrencyExchange>> {
    core::client::DukascopyClient::get_exchange_rates_range(pair, start, end, interval).await
}

// ============================================================================
// Market hours API
// ============================================================================

pub use market::{get_market_status, is_market_open, is_weekend, MarketStatus};

// ============================================================================
// Advanced API module
// ============================================================================

/// Advanced API for power users who need fine-grained control.
///
/// This module exposes internal types for:
/// - Custom client configuration (cache size, timeouts)
/// - Custom instrument configurations (for new/exotic instruments)
/// - Low-level parsing utilities
///
/// # Example
/// ```
/// use dukascopy_fx::advanced::{DukascopyClientBuilder, InstrumentConfig};
///
/// let client = DukascopyClientBuilder::new()
///     .cache_size(500)
///     .timeout_secs(60)
///     .with_instrument_config("BTC", "USD", InstrumentConfig::new(100.0, 2))
///     .build();
/// ```
pub mod advanced {
    pub use crate::core::client::{
        ClientConfig, ConfiguredClient, DukascopyClient, DukascopyClientBuilder,
        DEFAULT_CACHE_SIZE, DEFAULT_MAX_IDLE_CONNECTIONS, DEFAULT_TIMEOUT_SECS, DUKASCOPY_BASE_URL,
    };
    pub use crate::core::instrument::{
        resolve_instrument_config, CurrencyCategory, DefaultInstrumentProvider,
        HasInstrumentConfig, InstrumentConfig, InstrumentProvider, OverrideInstrumentProvider,
        DIVISOR_2_DECIMALS, DIVISOR_3_DECIMALS, DIVISOR_5_DECIMALS,
    };
    pub use crate::core::parser::{DukascopyParser, ParsedTick, TICK_SIZE_BYTES};
    pub use crate::market::last_available_tick_time;
}

// ============================================================================
// Prelude module
// ============================================================================

/// Prelude module - import everything commonly needed.
///
/// ```
/// use dukascopy_fx::prelude::*;
/// ```
pub mod prelude {
    pub use crate::api::{download, download_range, Ticker};
    pub use crate::error::DukascopyError;
    pub use crate::market::{is_market_open, is_weekend, MarketStatus};
    pub use crate::models::{CurrencyExchange, CurrencyPair};
    pub use crate::time::{
        date, datetime, days_ago, hours_ago, now, weeks_ago, DateTime, Duration, Utc,
    };
    pub use crate::{datetime, ticker};
    pub use crate::{get_rate, get_rate_for_pair, get_rates_range, get_rates_range_for_pair};
    pub use crate::{Error, Result};
}
