# dukascopy-fx

Library-first Rust crate for Dukascopy historical market data (FX, metals, indices, equities).

[![Crates.io](https://img.shields.io/crates/v/dukascopy-fx.svg)](https://crates.io/crates/dukascopy-fx)
[![Documentation](https://docs.rs/dukascopy-fx/badge.svg)](https://docs.rs/dukascopy-fx)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## 30-Second Quickstart

```toml
[dependencies]
dukascopy-fx = "0.5.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use dukascopy_fx::{Ticker, time::now};

#[tokio::main]
async fn main() -> dukascopy_fx::Result<()> {
    let ticker = Ticker::try_new("EUR", "USD")?;
    let rate = ticker.rate_at(now()).await?;
    println!("{} @ {}", rate.rate, rate.timestamp);
    Ok(())
}
```

## Why teams choose this crate

- yfinance-like ergonomics (`Ticker`, `history`, `rate_at`)
- unified request model (`RateRequest`) for pair/symbol flows
- strict + explicit parse modes (`RequestParseMode`)
- typed period API (`Period`) and tuned batch concurrency
- deduplicated concurrent fetches + parsed-tick cache reuse for better throughput
- checkpoint-driven incremental updates
- CLI fetcher for repeatable data jobs
- dataframe-friendly interop helpers (`flatten_rows`, `FlatExchangeRow`)

## What's New in 0.5.1

- Faster hot paths (request parsing, pair construction, mixed high-load scenarios).
- Safer cache-miss concurrency via singleflight with cancellation-safe wakeups.
- New client tuning knobs: `parsed_tick_cache_size(...)` and `max_decompression_jobs(...)`.
- Added controlled HTTP load/stress testing guide (`docs/LOAD_TESTING.md`) and `k6` script.

## Copy-Paste Workflows

### 1) Live fetch (library)

```bash
cargo run --example live_fetch
```

### 2) Incremental sync with checkpoints (library)

```bash
cargo run --example incremental_checkpoint
```

### 3) CSV/Parquet pipeline

```bash
# CSV (default features)
cargo run --example csv_parquet_pipeline

# Parquet sink enabled
cargo run --example csv_parquet_pipeline --features sinks-parquet
```

## Feature Matrix

| Capability | Default build | `--features sinks-parquet` |
|---|---|---|
| Library fetch API (`Ticker`, `get_rate*`) | Yes | Yes |
| CSV sink (`CsvSink`) | Yes | Yes |
| Parquet sink (`ParquetSink`) | No | Yes |
| `fx_fetcher` backfill/update to CSV | Yes | Yes |
| `fx_fetcher` backfill/update to Parquet | No | Yes |
| `fx_fetcher export` CSV -> Parquet | No | Yes |

## CLI Quickstart (`fx_fetcher`)

```bash
# discover/update universe
cargo run --bin fx_fetcher -- sync-universe --dry-run

# initial backfill
cargo run --bin fx_fetcher -- backfill \
  --symbols EURUSD,GBPUSD \
  --period 30d \
  --interval 1h \
  --out data/fx.csv

# incremental update
cargo run --bin fx_fetcher -- update \
  --symbols EURUSD,GBPUSD \
  --lookback 7d \
  --interval 1h \
  --checkpoint .state/checkpoints.json \
  --out data/fx.csv

# machine-readable output for automation
cargo run --bin fx_fetcher -- --json list-instruments

# config-driven defaults
cargo run --bin fx_fetcher -- --config config/fx_fetcher.example.toml backfill --symbols EURUSD
```

`fx_fetcher` global flags:
- `--config PATH.toml`: load command defaults from TOML file
- `--json`: emit JSON summary/errors for CI scripts and wrappers

## API Highlights

Most-used functions:
- `get_rate(from, to, timestamp)`
- `get_rate_for_request(&RateRequest, timestamp)`
- `get_rate_for_input(input, timestamp)`
- `get_rate_for_input_with_mode(input, RequestParseMode, timestamp)`
- `get_rate_for_symbol(symbol, timestamp)`
- `get_rate_in_quote(symbol, quote, timestamp)`
- `get_rates_range(from, to, start, end, interval)`

Most-used `Ticker` methods:
- `Ticker::try_new(from, to)`
- `Ticker::parse("EUR/USD")`
- `rate()`, `rate_at(timestamp)`
- `history("1w")`, `history_period(Period::Weeks(1))`
- `interval(Duration::minutes(30))`
- `fetch_incremental(&store, lookback)`

Advanced client configuration:
- `DukascopyClientBuilder::new()`
- `default_quote_currency("USD")`
- `pair_resolution_mode(...)`
- `conversion_mode(...)`, `bridge_currencies(...)`
- `code_alias("AAPL", "AAPLUS")`
- `max_in_flight_requests(...)`, `max_download_concurrency(...)`
- `parsed_tick_cache_size(...)`, `max_decompression_jobs(...)`
- `max_at_or_before_backtrack_hours(...)`

## FAQ (common issues)

`Missing command` / `Unknown option`
- run `cargo run --bin fx_fetcher -- --help`
- commands use strict flag validation (unknown flags are errors)

`backfill/update` fails with output-mode error
- pass exactly one mode: `--out PATH` (persist) or `--no-output` (dry fetch, no checkpoint advance)

`Parquet sink requires feature`
- build with `--features sinks-parquet`

`Invalid currency code`
- accepted code format is alphanumeric, length `2..12`
- prefer checked constructors: `Ticker::try_new`, `CurrencyPair::try_new`

`DataNotFound` / sparse timestamps
- timestamp may be outside available history
- weekends/market closure can require earlier timestamp

## Testing

Default:

```bash
cargo test --lib
cargo test --test public_api_offline_test
cargo test --bin fx_fetcher
```

Live integration (opt-in):

```bash
LIVE_TESTS=1 cargo test --test integration_test
```

## Project docs

- Changelog (release history): [`CHANGELOG.md`](CHANGELOG.md)
- API stability policy: [`docs/API_STABILITY.md`](docs/API_STABILITY.md)
- Benchmark methodology: [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md)
- CLI config reference: [`docs/CLI_CONFIG.md`](docs/CLI_CONFIG.md)
- Dataframe integrations: [`docs/INTEGRATIONS.md`](docs/INTEGRATIONS.md)
- Roadmap: [`ROADMAP.md`](ROADMAP.md)

## License

MIT
