//! Demonstrates writing fetched data to CSV and (optionally) Parquet sink.

use dukascopy_fx::storage::sink::{CsvSink, DataSink};
use dukascopy_fx::Ticker;

#[tokio::main]
async fn main() -> dukascopy_fx::Result<()> {
    let ticker = Ticker::try_new("EUR", "USD")?;
    let rows = ticker.history("1d").await?;

    let mut csv = CsvSink::open("data/example/fx.csv")?;
    let _ = csv.write_batch(&ticker.symbol(), &rows)?;
    csv.flush()?;
    println!("csv rows={} path=data/example/fx.csv", rows.len());

    #[cfg(feature = "sinks-parquet")]
    {
        use dukascopy_fx::storage::sink::ParquetSink;

        let mut parquet = ParquetSink::open("data/example/fx.parquet")?;
        let _ = parquet.write_batch(&ticker.symbol(), &rows)?;
        parquet.flush()?;
        println!("parquet rows={} path=data/example/fx.parquet", rows.len());
    }

    #[cfg(not(feature = "sinks-parquet"))]
    {
        println!("parquet skipped (build with --features sinks-parquet)");
    }

    Ok(())
}
