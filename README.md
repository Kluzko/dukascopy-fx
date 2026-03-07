# dukascopy-fx

Library-first Rust fetcher for Dukascopy historical market data.

This crate is built mainly as a **library** for quant/research pipelines and apps.  
CLI tooling exists, but is secondary.

[![Crates.io](https://img.shields.io/crates/v/dukascopy-fx.svg)](https://crates.io/crates/dukascopy-fx)
[![Documentation](https://docs.rs/dukascopy-fx/badge.svg)](https://docs.rs/dukascopy-fx)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Why this crate

- Simple yfinance-like API (`Ticker`, `history`, `rate_at`)
- Works for FX pairs and single symbols (equities/indices/metals)
- Automatic instrument scaling (standard FX, JPY, metals, index-like)
- Built-in retry/backoff, request limiting, LRU cache
- Incremental update support with checkpoints
- Universe catalog with aliases (e.g. `AAPL -> AAPLUS`)

## Installation

```toml
[dependencies]
dukascopy-fx = "0.5.0"
tokio = { version = "1", features = ["full"] }
```

## Quick Start (recommended)

```rust
use dukascopy_fx::{Ticker, datetime};

#[tokio::main]
async fn main() -> dukascopy_fx::Result<()> {
    let ticker = Ticker::try_new("EUR", "USD")?;

    let latest = ticker.rate().await?;
    println!("latest EUR/USD: {}", latest.rate);

    let one_week = ticker.history("1w").await?;
    println!("rows: {}", one_week.len());

    let at_time = ticker.rate_at(datetime!(2024-01-15 14:30 UTC)).await?;
    println!("at time: {}", at_time.rate);

    Ok(())
}
```

## API Guide

### 1) Single symbol or pair with one input

Use unified parsing when user input may be mixed (`AAPL` or `EUR/USD`).

```rust
use dukascopy_fx::{
    get_rate_for_input,
    get_rate_for_input_with_mode,
    get_rate_for_request,
    RateRequest,
    RequestParseMode,
};
use dukascopy_fx::time::now;

let fx = get_rate_for_input("EUR/USD", now()).await?;
let stock = get_rate_for_input("AAPL", now()).await?;
let strict_pair = get_rate_for_input_with_mode("EURUSD", RequestParseMode::PairOnly, now()).await?;

let req = RateRequest::symbol("MSFT")?;
let msft = get_rate_for_request(&req, now()).await?;
```

Parsing rules:
- input containing `/` -> explicit pair
- 6-letter FX shorthand (e.g. `EURUSD`, `XAUUSD`) -> explicit pair
- other input -> single symbol

### 2) Ticker API

```rust
use dukascopy_fx::{Period, Ticker, ticker};
use dukascopy_fx::time::Duration;

let t1 = Ticker::try_new("EUR", "USD")?;
let t2: Ticker = "GBP/JPY".parse()?;
let t3 = ticker!("XAU/USD");

let month = t1.history("1mo").await?;
let typed = t1.history_period(Period::Weeks(1)).await?;
let day_30m = t1.interval(Duration::minutes(30)).history("1d").await?;
```

### 3) Function API

```rust
use dukascopy_fx::{get_rate, get_rate_in_quote, get_rates_range, datetime};
use dukascopy_fx::time::Duration;

let eurusd = get_rate("EUR", "USD", datetime!(2024-01-15 14:30 UTC)).await?;
let aapl_pln = get_rate_in_quote("AAPL", "PLN", datetime!(2024-01-15 14:30 UTC)).await?;

let series = get_rates_range(
    "EUR",
    "USD",
    datetime!(2024-01-15 10:00 UTC),
    datetime!(2024-01-15 18:00 UTC),
    Duration::hours(1),
).await?;
```

### 4) Batch download

```rust
use dukascopy_fx::{download_with_concurrency, Ticker};

let tickers = vec![
    Ticker::eur_usd(),
    Ticker::gbp_usd(),
    Ticker::usd_jpy(),
    Ticker::xau_usd(),
];

let batch = download_with_concurrency(&tickers, "1w", 4).await?;
```

### 5) Incremental updates (checkpoint)

```rust
use dukascopy_fx::{FileCheckpointStore, Ticker};
use dukascopy_fx::time::Duration;

let store = FileCheckpointStore::open(".state/checkpoints.json")?;
let ticker = Ticker::try_new("EUR", "USD")?.interval(Duration::hours(1));

let rows = ticker.fetch_incremental(&store, Duration::days(7)).await?;
println!("fetched {} rows", rows.len());
```

## Public API (0.5.0)

Most-used free functions:
- `get_rate(from, to, timestamp)`
- `get_rate_for_pair(&CurrencyPair, timestamp)`
- `get_rate_for_request(&RateRequest, timestamp)`
- `get_rate_for_input(input, timestamp)`
- `get_rate_for_input_with_mode(input, RequestParseMode, timestamp)`
- `get_rate_for_symbol(symbol, timestamp)`
- `get_rate_in_quote(symbol, quote, timestamp)`
- `get_rates_range(from, to, start, end, interval)`
- `get_rates_range_for_pair(&CurrencyPair, start, end, interval)`

Most-used `Ticker` methods:
- `Ticker::try_new(from, to)` (validated)
- `Ticker::new(from, to)` (unchecked convenience)
- `Ticker::parse("EUR/USD")`
- `rate()`, `rate_at(timestamp)`
- `history("1w")`, `history_range(start, end)`
- `history_period(Period::Weeks(1))`
- `interval(Duration::minutes(30))`
- `fetch_incremental(&store, lookback)`

Most-used builder/client methods:
- `DukascopyClientBuilder::new()`
- `default_quote_currency("USD")` (spójna nazwa, brak `set_currency`)
- `pair_resolution_mode(...)`
- `conversion_mode(...)`, `bridge_currencies(...)`
- `code_alias("AAPL", "AAPLUS")`
- `max_download_concurrency(...)`
- `build()`
- `ConfiguredClient::get_exchange_rate_for_symbol(...)`
- `ConfiguredClient::get_exchange_rate_in_quote(...)`
- `max_at_or_before_backtrack_hours(...)`

Batch helpers:
- `download(...)`, `download_with_concurrency(...)`
- `download_range(...)`, `download_range_with_concurrency(...)`
- `download_incremental(...)`, `download_incremental_with_concurrency(...)`

## Advanced Client (power users)

Use `advanced` API when you need explicit control over resolution, aliases, quote defaults, and custom instrument configs.

```rust
use dukascopy_fx::advanced::{
    DukascopyClientBuilder,
    InstrumentConfig,
    PairResolutionMode,
};

let client = DukascopyClientBuilder::new()
    .cache_size(500)
    .timeout_secs(60)
    .default_quote_currency("USD")
    .pair_resolution_mode(PairResolutionMode::ExplicitOrDefaultQuote)
    .code_alias("AAPL", "AAPLUS")
    .with_instrument_config("BTC", "USD", InstrumentConfig::new(100.0, 2))
    .build();

let _ = client;
```

## Instrument Coverage

Typical instrument families:

| Type | Typical Divisor | Typical Decimals | Examples |
|------|------------------|------------------|----------|
| Standard FX | 100,000 | 5 | EUR/USD, GBP/USD |
| JPY pairs | 1,000 | 3 | USD/JPY, EUR/JPY |
| Metals | 1,000 | 3 | XAU/USD, XAG/USD |
| Index/equity-like | 1,000 (common) | 2 | USA500IDX/USD, AAPLUS/USD |

Universe file (`config/universe.json`) supports:
- instrument definitions (`symbol`, `base`, `quote`, `asset_class`, etc.)
- alias mapping (`code_aliases`)
- alias chains (`SP500 -> US500 -> USA500IDX`)

## Optional CLI (fetcher workflows)

Binary: `fx_fetcher`

Typical flow:

```bash
# 1) sync/refresh universe
cargo run --bin fx_fetcher -- sync-universe --dry-run

# 2) initial backfill
cargo run --bin fx_fetcher -- backfill \
  --symbols EURUSD,GBPUSD \
  --period 90d \
  --interval 1h \
  --out data/fx.parquet

# 3) incremental update
cargo run --bin fx_fetcher -- update \
  --symbols EURUSD,GBPUSD \
  --lookback 7d \
  --interval 1h \
  --checkpoint .state/checkpoints.json \
  --out data/fx.parquet

# explicit no-output mode (checkpoints won't be updated)
cargo run --bin fx_fetcher -- backfill \
  --symbols EURUSD \
  --period 7d \
  --no-output
```

Core commands:
- `list-instruments`
- `backfill`
- `update`
- `sync-universe`
- `export`

Notes:
- `backfill`/`update` require explicit output mode: `--out PATH` or `--no-output`
- `export` accepts `--has-headers` for CSV files with header row

## Feature Flags

- default: `logging`
- `sinks-parquet`: enables `ParquetSink` (and `fx_fetcher` binary)

Examples:

```bash
# library-only minimal build
cargo build

# enable parquet sink API and CLI binary
cargo build --features sinks-parquet
```

## Data Notes

- Historical depth depends on instrument (major FX often available since 2003)
- Data is fetched from hourly files; latest complete data is usually delayed by ~1 hour
- Weekend handling adjusts requests to last available trading timestamp

## Testing

By default, `cargo test` runs unit/offline tests. Live integration tests are opt-in:

```bash
LIVE_TESTS=1 cargo test --test integration_test
```

## Troubleshooting

`Invalid currency code`
- use alphanumeric codes, length `2..12`
- for market symbols, use canonical code or alias map

`DataNotFound`
- requested timestamp may be outside available history
- market may be closed
- instrument may not have data for that timeframe

`No conversion route`
- set conversion mode to synthetic and configure bridge currencies in advanced client

## License

MIT
