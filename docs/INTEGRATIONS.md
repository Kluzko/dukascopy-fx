# Integrations (Polars / Pandas)

This crate supports two practical integration patterns:

1. Use built-in CSV/Parquet sinks (`CsvSink`, `ParquetSink` with feature flag).
2. Use in-memory flat adapter (`FlatExchangeRow`) via `flatten_row(s)`.

## Rust -> Flat rows

```rust
use dukascopy_fx::{flatten_rows, Ticker};

# async fn example() -> dukascopy_fx::Result<()> {
let ticker = Ticker::try_new("EUR", "USD")?;
let rows = ticker.history("1d").await?;
let flat = flatten_rows(&ticker.symbol(), &rows);

// serialize to JSON for downstream Python/ETL jobs
let json = serde_json::to_string(&flat)?;
println!("{}", json);
# Ok(())
# }
```

## Pandas pipeline

```python
import pandas as pd

# CSV from CsvSink/fx_fetcher
csv_df = pd.read_csv(
    "data/fx.csv",
    names=[
        "symbol",
        "base",
        "quote",
        "timestamp",
        "rate",
        "bid",
        "ask",
        "bid_volume",
        "ask_volume",
    ],
)
csv_df["timestamp"] = pd.to_datetime(csv_df["timestamp"], utc=True)

# Parquet dataset from ParquetSink (directory with part-*.parquet)
parquet_df = pd.read_parquet("data/fx.parquet")
```

## Polars pipeline

```python
import polars as pl

csv_df = pl.read_csv(
    "data/fx.csv",
    has_header=False,
    new_columns=[
        "symbol",
        "base",
        "quote",
        "timestamp",
        "rate",
        "bid",
        "ask",
        "bid_volume",
        "ask_volume",
    ],
)

parquet_df = pl.read_parquet("data/fx.parquet")
```

## Notes

- Decimal values are stored as strings in `FlatExchangeRow` to preserve precision.
- For numeric analytics, cast rate/bid/ask to decimal/float in your dataframe engine.
- Parquet output requires `--features sinks-parquet`.
