//! Incremental fetch example using checkpoint store.

use dukascopy_fx::time::Duration;
use dukascopy_fx::{FileCheckpointStore, Ticker};

#[tokio::main]
async fn main() -> dukascopy_fx::Result<()> {
    let store = FileCheckpointStore::open(".state/checkpoints.json")?;
    let ticker = Ticker::try_new("EUR", "USD")?.interval(Duration::hours(1));

    let rows = ticker.fetch_incremental(&store, Duration::days(7)).await?;
    println!(
        "symbol={} rows={} checkpoint_key={}",
        ticker.symbol(),
        rows.len(),
        ticker.checkpoint_key()
    );
    Ok(())
}
