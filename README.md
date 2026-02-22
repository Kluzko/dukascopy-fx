# dukascopy-fx

A production-ready Rust library for fetching **historical forex data** from **Dukascopy**, inspired by Python's yfinance.

[![Crates.io](https://img.shields.io/crates/v/dukascopy-fx.svg)](https://crates.io/crates/dukascopy-fx)
[![Documentation](https://docs.rs/dukascopy-fx/badge.svg)](https://docs.rs/dukascopy-fx)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- **yfinance-style API** - Familiar `Ticker` object with `history()` method
- **Period strings** - Use `"1d"`, `"1w"`, `"1mo"`, `"1y"` for easy time ranges
- **Built-in time utilities** - No need to add chrono separately
- **Type-safe** - Strong types for currency pairs, rates, and errors
- **Automatic handling** - JPY pairs, metals, weekends handled transparently
- **Incremental fetch** - Checkpoint-based updates for fetcher pipelines
- **Instrument catalog** - Load universe from JSON (`config/universe.json`)
- **Universe sync CLI** - Discover instruments from public Dukascopy listings and merge safely
- **Alias + default quote support** - Resolve symbols like `AAPL -> AAPLUS`, request symbol-only rates with default quote
- **Free data** - No API keys required, data from 2003+

## Installation

```toml
[dependencies]
dukascopy-fx = "0.3"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use dukascopy_fx::{Ticker, datetime};

#[tokio::main]
async fn main() -> dukascopy_fx::Result<()> {
    // Create a ticker - yfinance style!
    let ticker = Ticker::new("EUR", "USD");

    // Get recent rate
    let rate = ticker.rate().await?;
    println!("EUR/USD: {}", rate.rate);

    // Get last week of hourly data
    let history = ticker.history("1w").await?;
    println!("Got {} records", history.len());

    // Get rate at specific time
    let rate = ticker.rate_at(datetime!(2024-01-15 14:30 UTC)).await?;
    println!("Rate at 2024-01-15: {}", rate.rate);

    Ok(())
}
```

## Usage

### Ticker API (Recommended)

```rust
use dukascopy_fx::{Ticker, datetime};

// Create tickers - multiple ways
let eur_usd = Ticker::new("EUR", "USD");
let gold = Ticker::xau_usd();              // Convenience constructor
let ticker: Ticker = "GBP/JPY".parse()?;   // Parse from string
let ticker = ticker!("USD/CHF");           // Using macro

// Get historical data with period strings
let daily = ticker.history("1d").await?;    // Last 24 hours
let weekly = ticker.history("1w").await?;   // Last 7 days
let monthly = ticker.history("1mo").await?; // Last 30 days
let yearly = ticker.history("1y").await?;   // Last 365 days

// Custom date range
use dukascopy_fx::time::{days_ago, weeks_ago};
let history = ticker.history_range(weeks_ago(2), days_ago(1)).await?;

// Change sampling interval (default: 1 hour)
use dukascopy_fx::time::Duration;
let ticker_30min = Ticker::new("EUR", "USD").interval(Duration::minutes(30));
let history = ticker_30min.history("1d").await?; // ~48 records instead of ~24
```

### Batch Download

```rust
use dukascopy_fx::{Ticker, download};

let tickers = vec![
    Ticker::eur_usd(),
    Ticker::gbp_usd(),
    Ticker::usd_jpy(),
    Ticker::xau_usd(),
];

let data = download(&tickers, "1w").await?;

for (ticker, rates) in data {
    println!("{}: {} records", ticker.symbol(), rates.len());
}
```

### Incremental Fetching (Checkpoint-Based)

```rust
use dukascopy_fx::{Ticker, FileCheckpointStore};
use dukascopy_fx::time::Duration;

let store = FileCheckpointStore::open(".state/checkpoints.json")?;
let ticker = Ticker::new("EUR", "USD").interval(Duration::hours(1));

// First run: bootstraps from lookback window
let rows = ticker.fetch_incremental(&store, Duration::days(7)).await?;
println!("Fetched {} rows", rows.len());

// Next runs: fetches only new data using persisted checkpoint
let rows = ticker.fetch_incremental(&store, Duration::days(7)).await?;
println!("Fetched {} rows", rows.len());
```

### Fetcher CLI

```bash
# List active symbols from universe
cargo run --bin fx_fetcher -- list-instruments

# One-time historical backfill (bounded concurrency)
cargo run --bin fx_fetcher -- backfill --symbols EURUSD,GBPUSD --period 30d --interval 1h --concurrency 8 --out data/fx.parquet

# Incremental update with checkpoints
cargo run --bin fx_fetcher -- update --symbols EURUSD,GBPUSD --lookback 7d --interval 1h --concurrency 8 --out data/fx.parquet

# Sync universe with public Dukascopy instrument list (new symbols inactive by default)
cargo run --bin fx_fetcher -- sync-universe --dry-run
cargo run --bin fx_fetcher -- sync-universe --activate-new

# Sync using custom source / path
cargo run --bin fx_fetcher -- sync-universe --source https://www.dukascopy-node.app --universe config/universe.json

# Convert existing CSV dump to Parquet
cargo run --bin fx_fetcher -- export --input data/fx.csv --out data/fx.parquet
```

`--out` supports `.csv` and `.parquet`.
For `.parquet`, output is an append-only parquet dataset directory with `part-*.parquet` files.

`sync-universe` behavior:
- reads public instrument listings (`sitemap.xml` + category pages)
- merges discovered symbols into your existing universe file
- keeps existing manual entries unchanged
- adds new symbols as inactive unless `--activate-new` is used

### Simple Function API

```rust
use dukascopy_fx::{get_rate, get_rates_range, datetime};
use dukascopy_fx::time::Duration;

// Single rate
let rate = get_rate("EUR", "USD", datetime!(2024-01-15 14:30 UTC)).await?;
println!("Rate: {}, Bid: {}, Ask: {}", rate.rate, rate.bid, rate.ask);

// Range of rates
let rates = get_rates_range(
    "EUR", "USD",
    datetime!(2024-01-15 10:00 UTC),
    datetime!(2024-01-15 18:00 UTC),
    Duration::hours(1),
).await?;
```

### Time Utilities

No need to add chrono to your dependencies - we re-export everything you need:

```rust
use dukascopy_fx::time::{DateTime, Utc, Duration, now, days_ago, weeks_ago};
use dukascopy_fx::datetime;

// Convenient time helpers
let current = now();
let yesterday = days_ago(1);
let last_week = weeks_ago(1);

// datetime! macro - multiple formats
let ts = datetime!(2024-01-15 14:30 UTC);      // Hour and minute
let ts = datetime!(2024-01-15 14:30:45 UTC);   // With seconds
let ts = datetime!(2024-01-15 UTC);             // Midnight
```

### Market Hours

```rust
use dukascopy_fx::{is_market_open, is_weekend, get_market_status, MarketStatus, datetime};

let saturday = datetime!(2024-01-06 12:00 UTC);

if is_weekend(saturday) {
    println!("It's the weekend");
}

if !is_market_open(saturday) {
    println!("Market is closed");
}

match get_market_status(saturday) {
    MarketStatus::Open => println!("Market is open"),
    MarketStatus::Weekend { reopens_at } => {
        println!("Closed for weekend, reopens {}", reopens_at);
    }
    MarketStatus::Holiday { name, reopens_at } => {
        println!("Holiday: {:?}, reopens {}", name, reopens_at);
    }
}
```

### Error Handling

```rust
use dukascopy_fx::{Ticker, DukascopyError, datetime};

let ticker = Ticker::new("EUR", "USD");

match ticker.rate_at(datetime!(2024-01-15 14:30 UTC)).await {
    Ok(rate) => println!("Rate: {}", rate.rate),
    Err(e) if e.is_retryable() => {
        // Network error, rate limit - safe to retry
        println!("Retryable error: {}", e);
    }
    Err(e) if e.is_not_found() => {
        // No data for this timestamp (too old, future date, etc.)
        println!("No data available: {}", e);
    }
    Err(e) if e.is_validation_error() => {
        // Invalid currency code
        println!("Invalid input: {}", e);
    }
    Err(e) => println!("Error: {}", e),
}
```

### Advanced: Custom Client Configuration

```rust
use dukascopy_fx::advanced::{DukascopyClientBuilder, InstrumentConfig};

let client = DukascopyClientBuilder::new()
    .cache_size(500)           // LRU cache entries (default: 100)
    .timeout_secs(60)          // HTTP timeout (default: 30)
    .default_quote_currency("USD")
    .code_alias("AAPL", "AAPLUS")
    .with_instrument_config(   // Custom instrument config
        "BTC", "USD",
        InstrumentConfig::new(100.0, 2),
    )
    .build();
```

## Good to Know

### Data Availability

- **Historical depth**: Major pairs available from 2003
- **Latest data**: ~1 hour delay (data is hourly)
- **Weekends**: No data from Friday 22:00 UTC to Sunday 22:00 UTC

### Weekend Handling

Request data for Saturday? The library automatically returns Friday's last available rate:

```rust
let saturday = datetime!(2024-01-06 15:00 UTC);
let rate = ticker.rate_at(saturday).await?;
// rate.timestamp will be Friday ~21:59 UTC, not Saturday
```

### Price Precision

Different instruments have different decimal places - handled automatically:

| Instrument | Example Rate | Decimals |
|------------|--------------|----------|
| EUR/USD | 1.08505 | 5 |
| USD/JPY | 154.325 | 3 |
| XAU/USD | 2645.50 | 2-3 |

### Caching

The library caches decompressed hourly data (LRU, 100 entries default). Requesting multiple timestamps within the same hour only fetches data once:

```rust
// These share the same cached hourly data file:
ticker.rate_at(datetime!(2024-01-15 14:05 UTC)).await?;
ticker.rate_at(datetime!(2024-01-15 14:30 UTC)).await?;
ticker.rate_at(datetime!(2024-01-15 14:55 UTC)).await?;
```

### Rate Limiting

Dukascopy may rate-limit aggressive requests. The client includes retry with exponential backoff and a global in-flight request limiter. For very large workloads, it's still a good idea to pace requests:

```rust
for ticker in tickers {
    let data = ticker.history("1mo").await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}
```

## Supported Instruments

| Type | Divisor | Decimals | Examples |
|------|---------|----------|----------|
| Standard Forex | 100,000 | 5 | EUR/USD, GBP/USD, AUD/USD, USD/PLN |
| JPY Pairs | 1,000 | 3 | USD/JPY, EUR/JPY, GBP/JPY |
| Metals | 1,000 | 3 | XAU/USD (Gold), XAG/USD (Silver) |
| RUB Pairs | 1,000 | 3 | USD/RUB, EUR/RUB |

`sync-universe` currently discovers ~1600 instruments from public listings (as of February 22, 2026), while keeping your local universe curated and explicit.

## Examples

```bash
cargo run --example basic            # Basic Ticker usage
cargo run --example advanced         # Batch downloads, market hours, error handling
cargo run --example batch_download   # Download multiple tickers
cargo run --example weekend_handling # Weekend data behavior
```

## Performance Tips

1. **Use period strings** - `history("1w")` is simpler than calculating dates
2. **Batch similar requests** - requests within same hour share cached data
3. **Check market hours** - avoid unnecessary requests during weekends
4. **Reuse tickers** - `Ticker` is cheap to clone

## Limitations

- **Historical only** - no real-time streaming
- **~1 hour delay** - data organized by completed hours
- **Weekend gaps** - no data Friday close to Sunday open
- **Rate limits** - Dukascopy may throttle aggressive requests

## License

MIT License - see [LICENSE](LICENSE)

## Disclaimer

This library uses Dukascopy's publicly available API for research and educational purposes. Not affiliated with Dukascopy Bank SA. Data provided "as-is" without warranty.

## Related Projects

- [dukascopy-node](https://github.com/Leo4815162342/dukascopy-node) - Node.js
- [duka](https://github.com/giuse88/duka) - Python
