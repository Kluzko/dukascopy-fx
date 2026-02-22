# dukascopy-fx

A Rust library and CLI fetcher for historical Dukascopy market data.

It is designed for two use-cases:
- as a **library** for quant/research code (`Ticker`, `download`, advanced client)
- as a **fetcher CLI** for repeatable backfill + incremental updates (`fx_fetcher`)

[![Crates.io](https://img.shields.io/crates/v/dukascopy-fx.svg)](https://crates.io/crates/dukascopy-fx)
[![Documentation](https://docs.rs/dukascopy-fx/badge.svg)](https://docs.rs/dukascopy-fx)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## What You Get

- yfinance-like `Ticker` API (`rate`, `rate_at`, `history`, `history_range`)
- period strings (`1d`, `1w`, `1mo`, `1y`) and configurable sampling interval
- automatic scaling for FX/JPY/metals/index-like instruments
- retry + backoff + request limiting in the client
- checkpoint-based incremental fetch for production pipelines
- universe catalog with aliases (`AAPL -> AAPLUS`, `SP500 -> USA500IDX`)
- universe sync command (`sync-universe`) from public instrument listings

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
    let ticker = Ticker::new("EUR", "USD");

    let latest = ticker.rate().await?;
    println!("latest EUR/USD: {}", latest.rate);

    let one_week = ticker.history("1w").await?;
    println!("records: {}", one_week.len());

    let at_time = ticker.rate_at(datetime!(2024-01-15 14:30 UTC)).await?;
    println!("at time: {}", at_time.rate);

    Ok(())
}
```

## Library Usage

### 1) Ticker API (recommended)

```rust
use dukascopy_fx::{Ticker, ticker};
use dukascopy_fx::time::Duration;

let t1 = Ticker::new("EUR", "USD");
let t2: Ticker = "GBP/JPY".parse()?;
let t3 = ticker!("XAU/USD");

let data = t1.history("1mo").await?;
let data_30m = t1.interval(Duration::minutes(30)).history("1d").await?;
```

### 2) Batch download

```rust
use dukascopy_fx::{Ticker, download};

let tickers = vec![
    Ticker::eur_usd(),
    Ticker::gbp_usd(),
    Ticker::usd_jpy(),
    Ticker::xau_usd(),
];

let batch = download(&tickers, "1w").await?;
```

### 3) Incremental updates with checkpoint

```rust
use dukascopy_fx::{Ticker, FileCheckpointStore};
use dukascopy_fx::time::Duration;

let store = FileCheckpointStore::open(".state/checkpoints.json")?;
let ticker = Ticker::new("EUR", "USD").interval(Duration::hours(1));

let rows = ticker.fetch_incremental(&store, Duration::days(7)).await?;
println!("fetched {} rows", rows.len());
```

### 4) Function API

```rust
use dukascopy_fx::{get_rate, get_rates_range, datetime};
use dukascopy_fx::time::Duration;

let r = get_rate("USD", "PLN", datetime!(2024-01-15 14:30 UTC)).await?;
let rs = get_rates_range(
    "EUR", "USD",
    datetime!(2024-01-15 10:00 UTC),
    datetime!(2024-01-15 18:00 UTC),
    Duration::hours(1),
).await?;
```

## Fetcher CLI

Binary: `fx_fetcher`

### Typical production flow

```bash
# 1) refresh universe
cargo run --bin fx_fetcher -- sync-universe --dry-run

# 2) initial backfill
cargo run --bin fx_fetcher -- backfill \
  --symbols EURUSD,GBPUSD \
  --period 90d \
  --interval 1h \
  --out data/fx.parquet

# 3) regular incremental update
cargo run --bin fx_fetcher -- update \
  --symbols EURUSD,GBPUSD \
  --lookback 7d \
  --interval 1h \
  --checkpoint .state/checkpoints.json \
  --out data/fx.parquet
```

### Commands

`list-instruments`
- Prints active instruments from universe file.

`backfill`
- Downloads historical range for selected instruments.

`update`
- Fetches only new rows using checkpoints (+ retry buffer).

`sync-universe`
- Discovers symbols from public listings (`sitemap.xml` + category pages)
- Merges into local universe
- Keeps existing entries
- Adds new symbols as inactive by default

`export`
- Converts CSV dump into parquet dataset.

### Important flags

`backfill` / `update`
- `--universe PATH`
- `--symbols EURUSD,GBPUSD`
- `--interval 1h`
- `--checkpoint PATH`
- `--out PATH.csv|PATH.parquet`
- `--concurrency N`

`backfill` only
- `--period 30d`

`update` only
- `--lookback 7d`

`sync-universe`
- `--universe PATH`
- `--source URL` (default: `https://www.dukascopy-node.app`)
- `--dry-run`
- `--activate-new`

## Universe Catalog

Universe file (`config/universe.json`) contains:
- `instruments`: explicit symbol definitions
- `code_aliases`: user-facing aliases to canonical codes

Example:

```json
{
  "instruments": [
    {
      "symbol": "EURUSD",
      "base": "EUR",
      "quote": "USD",
      "asset_class": "fx",
      "price_divisor": 100000.0,
      "decimal_places": 5,
      "active": true
    }
  ],
  "code_aliases": {
    "AAPL": "AAPLUS",
    "SP500": "USA500IDX"
  }
}
```

Rules:
- `symbol` must equal `base + quote`
- aliases are normalized to uppercase
- alias chains are supported (`SP500 -> US500 -> USA500IDX`)
- alias canonical targets are validated against catalog codes

## Advanced Client (aliases, default quote, custom config)

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
```

## Market Behavior and Data Notes

- Historical depth: major FX data available since 2003 (instrument-dependent)
- Data is fetched from hourly files; latest complete data is typically delayed by ~1 hour
- Weekend gap: no FX data from Friday close to Sunday open
- Weekend requests are adjusted to last available trading timestamp

## Supported Instrument Types

| Type | Typical Divisor | Typical Decimals | Examples |
|------|------------------|------------------|----------|
| Standard FX | 100,000 | 5 | EUR/USD, GBP/USD |
| JPY pairs | 1,000 | 3 | USD/JPY, EUR/JPY |
| Metals | 1,000 | 3 | XAU/USD, XAG/USD |
| Index/equity-like | 1,000 (common) | 2 | USA500IDX/USD, AAPLUS/USD |

`sync-universe` currently discovers ~1600 instruments from public listings (as of February 22, 2026).

## Troubleshooting

`Invalid currency code`
- ensure codes are alphanumeric and length 2..12
- for market symbols use canonical code or alias map

`DataNotFound`
- timestamp may be outside available history
- market may be closed
- instrument may not have data at requested timeframe

`No conversion route`
- set conversion mode to synthetic and configure bridge currencies in advanced client

Slow backfill
- lower `--concurrency` when remote side throttles
- run one-time backfill, then use incremental updates
- prefer parquet output for large datasets

## Examples

```bash
cargo run --example basic
cargo run --example advanced
cargo run --example batch_download
cargo run --example weekend_handling
```

## Limitations

- historical data fetcher (no low-latency streaming feed)
- remote throttling can occur on aggressive workloads
- data quality/availability depends on source instrument

## License

MIT License - see [LICENSE](LICENSE)

## Disclaimer

This project uses publicly available Dukascopy data endpoints.
It is not affiliated with Dukascopy Bank SA.
Data is provided as-is, without warranty.

## Related Projects

- [dukascopy-node](https://github.com/Leo4815162342/dukascopy-node) (Node.js)
- [duka](https://github.com/giuse88/duka) (Python)
