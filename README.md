# dukascopy-fx

A Rust library for fetching **historical forex (currency exchange) data** from **Dukascopy's** tick data API. This library provides a simple and efficient way to retrieve exchange rates with minute-level precision.

[![Crates.io](https://img.shields.io/crates/v/dukascopy-fx.svg)](https://crates.io/crates/dukascopy-fx)
[![Documentation](https://docs.rs/dukascopy-fx/badge.svg)](https://docs.rs/dukascopy-fx)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Why This Library?

Free APIs providing historical forex data with tick-level precision are hard to find. Dukascopy's API is free and offers high-precision tick data for a wide range of currency pairs, metals, and other instruments dating back to 2003.

**Key Benefits:**
- **Free Data**: No API keys or subscriptions required
- **High Precision**: Tick-level data with millisecond timestamps
- **Wide Coverage**: 500+ instruments including forex, metals, indices
- **Historical Depth**: Data available from 2003 for major pairs
- **Automatic Scaling**: Correct price divisors for all instrument types

## Features

- **Fetch Historical Forex Data**: Retrieve tick data for specific currency pairs and timestamps
- **Automatic Instrument Detection**: Correct price scaling for JPY pairs, metals (XAU, XAG), RUB pairs, and standard forex
- **Weekend Handling**: Automatically fetches last available tick from Friday for weekend timestamps
- **Caching**: LRU cache reduces redundant API requests
- **Market Hours Utilities**: Check if market is open, get next market open time
- **Error Handling**: Detailed error types with context and retry classification
- **Type-Safe Currency Pairs**: Parse from strings, validate codes, common pairs as constants
- **Batch Fetching**: Fetch rates over time ranges efficiently

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dukascopy-fx = "0.2.0"
tokio = { version = "1", features = ["full"] }
chrono = "0.4"
```

## Quick Start

```rust
use dukascopy_fx::{DukascopyFxService, CurrencyPair};
use chrono::{Utc, TimeZone};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a currency pair
    let pair = CurrencyPair::new("EUR", "USD");
    
    // Fetch exchange rate
    let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 0).unwrap();
    let exchange = DukascopyFxService::get_exchange_rate(&pair, timestamp).await?;
    
    println!("Rate: {} at {}", exchange.rate, exchange.timestamp);
    println!("Bid: {}, Ask: {}", exchange.bid, exchange.ask);
    println!("Spread: {}", exchange.spread());
    
    Ok(())
}
```

## Supported Instruments

The library automatically detects instrument types and applies correct price scaling:

| Type | Divisor | Decimals | Examples |
|------|---------|----------|----------|
| Standard Forex | 100,000 | 5 | EUR/USD, GBP/USD, AUD/USD, USD/PLN, EUR/CHF |
| JPY Pairs | 1,000 | 3 | USD/JPY, EUR/JPY, GBP/JPY, AUD/JPY |
| Metals | 1,000 | 3 | XAU/USD (Gold), XAG/USD (Silver), XAU/EUR |
| RUB Pairs | 1,000 | 3 | USD/RUB, EUR/RUB |

### Common Currency Pairs Available

**Major Pairs:** EUR/USD, GBP/USD, USD/JPY, USD/CHF, AUD/USD, USD/CAD, NZD/USD

**Cross Pairs:** EUR/GBP, EUR/JPY, GBP/JPY, EUR/CHF, EUR/AUD, GBP/CHF

**Exotic Pairs:** USD/PLN, USD/TRY, USD/ZAR, USD/MXN, EUR/PLN, USD/RUB

**Metals:** XAU/USD, XAG/USD, XAU/EUR, XAG/EUR

---

## API Reference

### CurrencyPair

The `CurrencyPair` struct represents a forex pair with type-safe construction:

```rust
use dukascopy_fx::CurrencyPair;

// Construction methods
let pair = CurrencyPair::new("EUR", "USD");           // From strings (auto-uppercase)
let pair = CurrencyPair::try_new("EUR", "USD")?;      // With validation
let pair: CurrencyPair = "EUR/USD".parse()?;          // Parse with slash
let pair: CurrencyPair = "EURUSD".parse()?;           // Parse without slash

// Predefined pairs for convenience
let pair = CurrencyPair::eur_usd();   // EUR/USD
let pair = CurrencyPair::gbp_usd();   // GBP/USD
let pair = CurrencyPair::usd_jpy();   // USD/JPY
let pair = CurrencyPair::usd_chf();   // USD/CHF
let pair = CurrencyPair::aud_usd();   // AUD/USD
let pair = CurrencyPair::usd_cad();   // USD/CAD
let pair = CurrencyPair::nzd_usd();   // NZD/USD
let pair = CurrencyPair::xau_usd();   // Gold
let pair = CurrencyPair::xag_usd();   // Silver

// Methods
pair.from()         // Source currency: "EUR"
pair.to()           // Target currency: "USD"
pair.as_symbol()    // Combined: "EURUSD"
pair.inverse()      // Reversed: CurrencyPair { USD, EUR }
format!("{}", pair) // Display: "EUR/USD"
```

### DukascopyFxService

The main service for fetching exchange rates:

```rust
use dukascopy_fx::{DukascopyFxService, CurrencyPair};
use chrono::{Duration, Utc, TimeZone};

let pair = CurrencyPair::eur_usd();
let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 0).unwrap();

// Fetch single rate
let exchange = DukascopyFxService::get_exchange_rate(&pair, timestamp).await?;

// Fetch rates over a time range
let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
let end = Utc.with_ymd_and_hms(2024, 1, 15, 18, 0, 0).unwrap();
let rates = DukascopyFxService::get_exchange_rates_range(
    &pair,
    start,
    end,
    Duration::hours(1),  // Hourly intervals
).await?;

// Get last tick of a specific hour
let exchange = DukascopyFxService::get_last_tick_of_hour(&pair, timestamp).await?;
```

### CurrencyExchange

The response structure containing rate information:

```rust
pub struct CurrencyExchange {
    pub pair: CurrencyPair,         // The currency pair
    pub rate: Decimal,              // Mid price (average of bid/ask)
    pub timestamp: DateTime<Utc>,   // Actual tick timestamp
    pub ask: Decimal,               // Ask (offer) price
    pub bid: Decimal,               // Bid price
    pub ask_volume: f32,            // Volume at ask
    pub bid_volume: f32,            // Volume at bid
}

// Methods
exchange.spread()       // Calculate spread: ask - bid
exchange.spread_pips()  // Spread in pips (instrument-aware)
```

---

## Market Hours

The forex market operates 24/5, from Sunday evening to Friday evening UTC:

| Period | Sunday Open (UTC) | Friday Close (UTC) |
|--------|-------------------|-------------------|
| Winter (Nov-Mar) | 22:00 | 22:00 |
| Summer (Mar-Nov) | 21:00 | 21:00 |

### Market Hours Utilities

```rust
use dukascopy_fx::{is_market_open, is_weekend, get_market_status, MarketStatus};
use chrono::{Utc, TimeZone};

let timestamp = Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap(); // Saturday

// Simple checks
if is_weekend(timestamp) {
    println!("It's the weekend");
}

if !is_market_open(timestamp) {
    println!("Market is closed");
}

// Detailed status with reopen time
match get_market_status(timestamp) {
    MarketStatus::Open => {
        println!("Market is open for trading");
    }
    MarketStatus::Weekend { reopens_at } => {
        println!("Market closed for weekend, reopens at {}", reopens_at);
    }
    MarketStatus::Holiday { name, reopens_at } => {
        println!("Market closed for {:?}, reopens at {}", name, reopens_at);
    }
}
```

### Weekend Handling

When you request data for a weekend timestamp, the library automatically returns the last available tick from Friday before market close:

```rust
// Request for Saturday - automatically gets Friday's last tick
let saturday = Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap();
let exchange = DukascopyFxService::get_exchange_rate(&pair, saturday).await?;

// exchange.timestamp will be Friday around 21:59 UTC
assert_eq!(exchange.timestamp.weekday(), chrono::Weekday::Fri);
```

---

## Error Handling

The library provides detailed error types with classification methods:

```rust
use dukascopy_fx::DukascopyError;

match DukascopyFxService::get_exchange_rate(&pair, timestamp).await {
    Ok(exchange) => {
        println!("Rate: {}", exchange.rate);
    }
    Err(e) => {
        // Check error type
        if e.is_retryable() {
            // Rate limit, timeout, network error - safe to retry
            println!("Retryable error: {}", e);
        } else if e.is_not_found() {
            // No data available for this timestamp/pair
            println!("Data not found: {}", e);
        } else if e.is_validation_error() {
            // Invalid currency code or request
            println!("Validation error: {}", e);
        } else {
            // Other error
            println!("Error: {}", e);
        }
    }
}
```

### Error Types

| Error | Description | Retryable |
|-------|-------------|-----------|
| `HttpError` | Network or HTTP errors | Yes |
| `RateLimitExceeded` | API rate limit hit | Yes |
| `Timeout` | Request timed out | Yes |
| `DataNotFound` | No data for timestamp/pair | No |
| `InvalidCurrencyCode` | Invalid currency code | No |
| `InvalidTickData` | Corrupted data | No |
| `LzmaError` | Decompression failed | No |

### Retry Pattern

```rust
async fn fetch_with_retry(
    pair: &CurrencyPair,
    timestamp: DateTime<Utc>,
    max_retries: u32,
) -> Result<CurrencyExchange, DukascopyError> {
    for attempt in 0..max_retries {
        match DukascopyFxService::get_exchange_rate(pair, timestamp).await {
            Ok(exchange) => return Ok(exchange),
            Err(e) if e.is_retryable() && attempt < max_retries - 1 => {
                tokio::time::sleep(Duration::from_millis(100 * (attempt as u64 + 1))).await;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
```

---

## Examples

The library includes several examples in the `examples/` directory:

### Basic Usage

```bash
cargo run --example basic
```

Demonstrates simple rate fetching for different currency pairs.

### Advanced Usage

```bash
cargo run --example advanced
```

Demonstrates:
- Fetching multiple pairs
- Different instrument types
- Market hours utilities
- Error handling patterns
- Spread analysis
- Time range fetching

### Batch Download

```bash
cargo run --example batch_download
```

Demonstrates efficient batch downloading of historical data with CSV export.

---

## Caching

The library uses an LRU (Least Recently Used) cache to minimize API requests:

- **Cache Size**: 100 entries (decompressed hourly data)
- **Cache Key**: Full URL (includes pair, date, hour)
- **Scope**: Process-global, shared across all calls

### Cache Management

```rust
use dukascopy_fx::DukascopyClient;

// Check cache size
let size = DukascopyClient::cache_len().await?;
println!("Cached entries: {}", size);

// Clear cache (force fresh data)
DukascopyClient::clear_cache().await?;
```

---

## Data Source Details

### URL Format

Data is fetched from Dukascopy's public tick data API:

```
https://datafeed.dukascopy.com/datafeed/{PAIR}/{YEAR}/{MONTH}/{DAY}/{HOUR}h_ticks.bi5
```

- `{PAIR}`: Combined pair symbol (e.g., "EURUSD")
- `{YEAR}`: 4-digit year
- `{MONTH}`: 0-indexed month (00-11)
- `{DAY}`: Day of month (01-31)
- `{HOUR}`: Hour (0-23)

### Binary Format

Files are LZMA compressed. After decompression, each tick is 20 bytes:

| Bytes | Type | Description |
|-------|------|-------------|
| 0-3 | u32 BE | Milliseconds from hour start |
| 4-7 | u32 BE | Ask price (raw, divide by divisor) |
| 8-11 | u32 BE | Bid price (raw, divide by divisor) |
| 12-15 | f32 BE | Ask volume |
| 16-19 | f32 BE | Bid volume |

### Data Availability

- **Start Date**: Varies by instrument (2003 for major pairs)
- **End Date**: Previous hour (data is hourly)
- **Frequency**: Every price change (tick-level)
- **Coverage**: ~500+ instruments

---

## Performance Tips

1. **Use Caching**: The library caches decompressed data. Avoid clearing cache unnecessarily.

2. **Batch Requests**: Use `get_exchange_rates_range()` for multiple timestamps in the same hour - it only fetches once.

3. **Avoid Weekends**: Check `is_weekend()` before making requests if you need current data.

4. **Handle Errors**: Use `is_retryable()` to implement retry logic for transient failures.

5. **Reuse Pairs**: `CurrencyPair` is cheap to clone. Create once and reuse.

---

## Limitations

- **Historical Only**: No real-time streaming data
- **Hourly Granularity**: Data is organized by hour; fetching spans multiple files
- **Rate Limits**: Dukascopy may rate-limit aggressive requests
- **No Guarantees**: Data accuracy depends on Dukascopy's service
- **Weekend Gaps**: No data from Friday close to Sunday open

---

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Run tests (`cargo test`)
4. Run lints (`cargo clippy`)
5. Format code (`cargo fmt`)
6. Commit your changes
7. Push to the branch
8. Open a Pull Request

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## Disclaimer

This library uses Dukascopy's publicly available tick data API for research and educational purposes. It is not affiliated with, endorsed by, or vetted by Dukascopy Bank SA. Use at your own risk.

**Important Notes:**
- Data is provided "as-is" without warranty
- Not suitable for production trading without validation
- Respect Dukascopy's terms of service
- Consider rate limiting your requests

---

## Related Projects

- [dukascopy-node](https://github.com/Leo4815162342/dukascopy-node) - Node.js library
- [duka](https://github.com/giuse88/duka) - Python downloader
- [go-duka](https://github.com/adyzng/go-duka) - Go downloader
