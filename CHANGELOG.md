# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-01-19

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
- Private fields with accessor methods (`from()`, `to()`)

#### Market Hours Utilities
- New `market` module for forex market hours
- `is_weekend()` - check if timestamp is Saturday/Sunday
- `is_market_open()` - check if forex market is open
- `get_market_status()` - get detailed market status with reopen time
- `last_trading_day()` - get last trading day before a date
- `last_available_tick_time()` - adjust timestamp to last available data
- `next_market_open()` - calculate next market open time
- `MarketStatus` enum with `Open`, `Weekend`, `Holiday` variants

#### Enhanced Parser
- New `ParsedTick` struct with helper methods (`mid_price()`, `spread()`)
- `parse_tick_with_config()` - parse tick with instrument-specific divisor
- `iter_ticks()` - iterator over ticks in data
- `tick_count()` - count ticks in data
- `TICK_SIZE_BYTES` constant (20 bytes)

#### CurrencyExchange Improvements
- Added `ask` and `bid` fields (previously only mid-price `rate`)
- `spread()` method
- `spread_pips()` method
- `Display` implementation

#### Client Improvements
- `build_url()` helper method for constructing Dukascopy API URLs
- `clear_cache()` method to force fresh data
- `cache_len()` method to check cache size
- HTTP timeout (30 seconds default) prevents hanging requests
- Proper handling of empty responses
- Extracted `map_http_error()` for cleaner error handling
- Centralized cache initialization with `get_cache()` helper

#### Error Handling Improvements
- `InvalidCurrencyCode` now includes `code` and `reason` fields
- `DataNotFoundFor` variant with `pair` and `timestamp` context
- `Timeout` variant with duration
- `CacheError` variant for cache issues
- `InvalidRequest` now includes message
- `is_retryable()` method
- `is_not_found()` method
- `is_validation_error()` method

#### Service Improvements
- `get_exchange_rates_range()` for fetching multiple rates
- `get_last_tick_of_hour()` for end-of-hour rates
- Data validation before parsing
- DRY refactoring - extracted helper methods

#### Testing
- 111 unit tests covering all modules
- 3 integration tests including JPY pair validation
- 8 doc tests
- Tests for edge cases (empty data, weekends, invalid input)
- Price conversion tests for different instrument types

### Changed

- **BREAKING**: `CurrencyPair` fields are now private (use `from()` and `to()` methods)
- **BREAKING**: `CurrencyExchange` now includes `ask` and `bid` fields
- `parse_tick()` is deprecated in favor of `parse_tick_with_config()`
- Weekend handling now uses `last_available_tick_time()` from market module
- Friday close time fixed to 21:00 UTC (was incorrectly 23:00)

### Fixed

- **Critical**: JPY pairs now use correct divisor (1,000 instead of 100,000)
- **Critical**: Metals (XAU, XAG) now use correct divisor (1,000)
- **Critical**: RUB pairs now use correct divisor (1,000)
- Data validation is now called before parsing
- Friday market close hour corrected (21:00/22:00 UTC depending on DST)
- Empty response handling (returns `DataNotFound` instead of caching empty data)
- Mutex poisoning handled with proper error messages
- HTTP timeout prevents hanging requests

### Removed

- Direct field access on `CurrencyPair` (use accessor methods instead)

## [0.1.0] - 2025-01-15

### Added

- Initial release
- Basic forex data fetching from Dukascopy
- LRU caching for API responses
- LZMA decompression
- Weekend detection (returns Friday data)
- Basic error handling

---

## Migration Guide: 0.1.0 → 0.2.0

### CurrencyPair Construction

```rust
// Old (0.1.0) - no longer compiles
let pair = CurrencyPair {
    from: "USD".to_string(),
    to: "PLN".to_string(),
};

// New (0.2.0) - use constructor
let pair = CurrencyPair::new("USD", "PLN");

// Or parse from string
let pair: CurrencyPair = "USD/PLN".parse().unwrap();
```

### Accessing CurrencyPair Fields

```rust
// Old (0.1.0)
println!("{}", pair.from);

// New (0.2.0)
println!("{}", pair.from());
```

### JPY/Metal Pair Prices

Prices for JPY pairs, metals (XAU, XAG), and RUB pairs are now correct. If you had workarounds for incorrect prices, remove them:

```rust
// Old workaround (remove this)
let corrected_rate = exchange.rate * 100;

// New (0.2.0) - prices are correct automatically
let rate = exchange.rate; // Already correct!
```

### Using Bid/Ask Prices

```rust
// New in 0.2.0
let exchange = DukascopyFxService::get_exchange_rate(&pair, timestamp).await?;
println!("Bid: {}, Ask: {}", exchange.bid, exchange.ask);
println!("Spread: {}", exchange.spread());
```
