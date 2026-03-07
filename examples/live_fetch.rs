//! Minimal live fetch example.

use dukascopy_fx::{time::now, Ticker};

#[tokio::main]
async fn main() -> dukascopy_fx::Result<()> {
    let ticker = Ticker::try_new("EUR", "USD")?;
    let rate = ticker.rate_at(now()).await?;

    println!(
        "symbol={} ts={} rate={}",
        ticker.symbol(),
        rate.timestamp,
        rate.rate
    );
    Ok(())
}
