# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

_No changes yet._

## [0.4.0] - 2026-02-23

### Added

#### Unified Request API (library-first)
- New `RateRequest` model for single-entry request handling:
  - `RateRequest::Pair(CurrencyPair)` for explicit pair requests (`EUR/USD`)
  - `RateRequest::Symbol(String)` for single-symbol requests (`AAPL`, `XAUUSD`)
- `RateRequest` parsing via `FromStr`:
  - input containing `/` is parsed as pair
  - other input is parsed as single symbol
- New helper functions:
  - `get_rate_for_request(&RateRequest, timestamp)`
  - `get_rate_for_input(&str, timestamp)`
- Added conversion traits and formatting support for `RateRequest`.

#### Fetcher Universe Sync
- New CLI command: `fx_fetcher sync-universe`
- Discovers instruments from public Dukascopy catalog pages (`sitemap.xml` + category pages)
- Supports `--dry-run` to preview changes without writing files
- Supports `--activate-new` to auto-enable newly discovered instruments
- Supports `--source URL` and `--universe PATH` for custom sync targets

#### Catalog Merge Safety
- Automatic merge of discovered instruments into `config/universe.json`
- Existing manual entries are preserved
- Newly discovered instruments default to `active=false` unless explicitly activated
- Deterministic JSON output ordering for stable diffs

#### Alias Resolution Improvements
- Added alias chain resolution support (e.g. `SP500 -> US500 -> USA500IDX`)
- Added loop-safe alias resolution behavior
- Added catalog validation for alias canonical targets (canonical code must exist in catalog)

#### Test Coverage
- Added tests for `RateRequest` parsing/validation and mixed request flows.
- Added tests for sitemap/category slug extraction
- Added tests for symbol split/inference and catalog merge behavior
- Added tests for alias chain resolution and alias canonical validation

### Changed

- `README` reorganized for clearer library usage flow (single symbol + pair support).
- Crate metadata improved (`description`, `keywords`, `categories`) for library-first discoverability.
- Fetcher CLI usage/help updated with `sync-universe` workflow.
- Alias handling in client/catalog now resolves to final canonical code consistently.

## [0.3.0] - 2026-01-20

### Added

#### yfinance-style Ticker API
- New `Ticker` struct with familiar yfinance-style interface
- `Ticker::new("EUR", "USD")` - create ticker from currency codes
- `Ticker::parse("EUR/USD")` - parse from string
- `Ticker::rate()` - get most recent rate
- `Ticker::rate_at(timestamp)` - get rate at specific time
- `Ticker::history("1w")` - get historical data with period strings
- `Ticker::history_range(start, end)` - get data for custom date range
- `Ticker::interval(duration)` - set sampling interval
- Convenience constructors: `eur_usd()`, `gbp_usd()`, `usd_jpy()`, `xau_usd()`, etc.
- `FromStr` implementation for parsing tickers

#### Period String Support
- `"1d"` - 1 day
- `"5d"` - 5 days
- `"1w"`, `"2w"` - weeks
- `"1mo"`, `"3mo"`, `"6mo"` - months
- `"1y"` - 1 year

#### Batch Download
- `download(&tickers, "1w")` - download multiple tickers with period
- `download_range(&tickers, start, end)` - download with custom range

#### Time Utilities Module
- Re-exports chrono types (`DateTime`, `Utc`, `Duration`, etc.)
- `now()` - current UTC time
- `days_ago(n)` - n days ago
- `weeks_ago(n)` - n weeks ago
- `hours_ago(n)` - n hours ago
- `datetime(year, month, day, hour, min, sec)` - create datetime
- `date(year, month, day)` - create date at midnight

#### Macros
- `datetime!(2024-01-15 14:30 UTC)` - concise datetime creation
- `datetime!(2024-01-15 14:30:45 UTC)` - with seconds
- `datetime!(2024-01-15 UTC)` - date only (midnight)
- `ticker!("EUR/USD")` - create ticker from string
- `ticker!("EUR", "USD")` - create ticker from codes

#### Code Organization
- New `core/` module for internal implementation
  - `core/client.rs` - merged HTTP client and service logic
  - `core/instrument.rs` - price scaling configuration
  - `core/parser.rs` - binary tick parsing
- New `api/` module for public interfaces
  - `api/ticker.rs` - Ticker API
- Clear separation of public vs internal code

#### Prelude Module
- `use dukascopy_fx::prelude::*` imports all commonly needed types

#### Enhanced Testing
- 17 integration tests hitting real Dukascopy API
- Price divisor verification tests (JPY, XAU, XAG, standard)
- Weekend handling tests
- Ticker API end-to-end tests
- 78 unit tests, 22 doc tests

### Changed

- **Simplified public API** - most users only need `Ticker`, `datetime!`, `download`
- **Reorganized code structure** - better folder organization
- **Reduced code size** - from ~4,445 to ~2,875 lines (35% reduction)
- **Merged client and service** - eliminated duplication between modules
- `DukascopyFxService` moved to internal, use `Ticker` or `get_rate()` instead
- Advanced features now in `dukascopy_fx::advanced` module

### Removed

- Old `service.rs` (merged into `core/client.rs`)
- Old `client.rs` (merged into `core/client.rs`)
- Old `instrument.rs` (moved to `core/instrument.rs`)
- Old `parser.rs` (moved to `core/parser.rs`)
- Old `ticker.rs` (moved to `api/ticker.rs`)
- Redundant re-exports in `core/mod.rs`

### Migration from 0.2.0

```rust
// Old (0.2.0)
use dukascopy_fx::{DukascopyFxService, CurrencyPair};
use chrono::{Utc, TimeZone};

let pair = CurrencyPair::new("EUR", "USD");
let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 0).unwrap();
let rate = DukascopyFxService::get_exchange_rate(&pair, timestamp).await?;

// New (0.3.0) - Ticker API
use dukascopy_fx::{Ticker, datetime};

let ticker = Ticker::new("EUR", "USD");
let rate = ticker.rate_at(datetime!(2024-01-15 14:30 UTC)).await?;

// Or even simpler
let history = ticker.history("1w").await?;
```

The old API still works via `get_rate()` and `get_rate_for_pair()` functions.

---

## [0.2.0] - 2026-01-19

### Added

#### Instrument Configuration System
- New `instrument` module with extensible price scaling configuration
- Automatic detection of instrument types (standard forex, JPY pairs, metals, RUB pairs)
- `InstrumentConfig` struct with predefined configurations (`STANDARD`, `JPY`, `METALS`, `RUB`, `INDEX`)
- `CurrencyCategory` enum for categorizing currencies
- `resolve_instrument_config()` function for automatic config resolution
- `HasInstrumentConfig` trait for types that have instrument configuration

#### CurrencyPair Improvements
- `CurrencyPair::new()` constructor with automatic uppercase conversion
- `CurrencyPair::try_new()` constructor with validation
- `FromStr` implementation - parse from "EUR/USD" or "EURUSD" formats
- `Display` implementation - formats as "EUR/USD"
- `Hash` implementation for use in HashMaps/HashSets
- `as_symbol()` method returns "EURUSD" format
- `inverse()` method returns reversed pair
- Predefined pairs: `eur_usd()`, `gbp_usd()`, `usd_jpy()`, `xau_usd()`, etc.

#### Market Hours Utilities
- New `market` module for forex market hours
- `is_weekend()` - check if timestamp is Saturday/Sunday
- `is_market_open()` - check if forex market is open
- `get_market_status()` - get detailed market status with reopen time
- `last_trading_day()` - get last trading day before a date
- `last_available_tick_time()` - adjust timestamp to last available data
- `MarketStatus` enum with `Open`, `Weekend`, `Holiday` variants

#### Enhanced Parser
- New `ParsedTick` struct with helper methods (`mid_price()`, `spread()`)
- `parse_tick_with_config()` - parse tick with instrument-specific divisor
- `iter_ticks()` - iterator over ticks in data
- `tick_count()` - count ticks in data

#### CurrencyExchange Improvements
- Added `ask` and `bid` fields
- `spread()` method
- `spread_pips()` method

#### Error Handling Improvements
- `InvalidCurrencyCode` with `code` and `reason` fields
- `is_retryable()`, `is_not_found()`, `is_validation_error()` methods

### Changed

- **BREAKING**: `CurrencyPair` fields are now private (use `from()` and `to()` methods)
- **BREAKING**: `CurrencyExchange` now includes `ask` and `bid` fields

### Fixed

- **Critical**: JPY pairs now use correct divisor (1,000 instead of 100,000)
- **Critical**: Metals (XAU, XAG) now use correct divisor (1,000)
- **Critical**: RUB pairs now use correct divisor (1,000)
- Friday market close hour corrected (21:00/22:00 UTC depending on DST)

---

## [0.1.0] - 2025-01-15

### Added

- Initial release
- Basic forex data fetching from Dukascopy
- LRU caching for API responses
- LZMA decompression
- Weekend detection (returns Friday data)
- Basic error handling
