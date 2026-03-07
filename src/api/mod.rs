//! Public API for fetching forex data.
//!
//! This module provides the user-facing API including:
//! - `Ticker` - yfinance-style ticker for fetching exchange rates
//! - `download` - batch download function for multiple tickers

mod ticker;

pub use ticker::{
    download, download_incremental, download_incremental_with_concurrency, download_range,
    download_range_with_concurrency, download_with_concurrency, Period, Ticker,
    DEFAULT_DOWNLOAD_CONCURRENCY,
};
