//! # dukascopy-fx
//!
//! A Rust library for fetching historical forex (currency exchange) data from Dukascopy's
//! tick data API with minute-level precision.
//!
//! ## Features
//!
//! - **Fetch Historical Forex Data**: Retrieve tick data for specific currency pairs and timestamps
//! - **Automatic Instrument Detection**: Correct price scaling for JPY pairs, metals, and standard forex
//! - **Weekend Handling**: Automatically fetches last available tick from Friday for weekend timestamps
//! - **Caching**: LRU cache reduces redundant API requests
//! - **Error Handling**: Detailed error types with context
//!
//! ## Quick Start
//!
//! ```no_run
//! use dukascopy_fx::{DukascopyFxService, CurrencyPair};
//! use chrono::{Utc, TimeZone};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a currency pair
//! let pair = CurrencyPair::new("EUR", "USD");
//!
//! // Or parse from string
//! let pair: CurrencyPair = "USD/JPY".parse()?;
//!
//! // Fetch exchange rate
//! let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 0).unwrap();
//! let exchange = DukascopyFxService::get_exchange_rate(&pair, timestamp).await?;
//!
//! println!("Rate: {} at {}", exchange.rate, exchange.timestamp);
//! println!("Bid: {}, Ask: {}, Spread: {}", exchange.bid, exchange.ask, exchange.spread());
//! # Ok(())
//! # }
//! ```
//!
//! ## Supported Instruments
//!
//! The library automatically detects and applies correct price scaling:
//!
//! | Type | Divisor | Examples |
//! |------|---------|----------|
//! | Standard Forex | 100,000 | EUR/USD, GBP/USD, AUD/USD |
//! | JPY Pairs | 1,000 | USD/JPY, EUR/JPY, GBP/JPY |
//! | Metals | 1,000 | XAU/USD, XAG/USD |
//! | RUB Pairs | 1,000 | USD/RUB, EUR/RUB |

pub mod client;
pub mod error;
pub mod instrument;
pub mod market;
pub mod models;
pub mod parser;
pub mod service;

// Re-export main types
pub use client::DukascopyClient;
pub use error::DukascopyError;
pub use instrument::{resolve_instrument_config, HasInstrumentConfig, InstrumentConfig};
pub use market::{get_market_status, is_market_open, is_weekend, MarketStatus};
pub use models::{CurrencyExchange, CurrencyPair};
pub use parser::{DukascopyParser, ParsedTick, TICK_SIZE_BYTES};
pub use service::DukascopyFxService;
