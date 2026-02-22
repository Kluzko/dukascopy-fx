//! Batch download example - yfinance style
//!
//! Shows how to download data for multiple currency pairs efficiently.

use dukascopy_fx::{datetime, download, download_range, Ticker};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> dukascopy_fx::Result<()> {
    env_logger::init();

    println!("=== Batch Download Example ===\n");

    // Define tickers
    let tickers = vec![
        Ticker::eur_usd(),
        Ticker::gbp_usd(),
        Ticker::usd_jpy(),
        Ticker::usd_chf(),
        Ticker::aud_usd(),
        Ticker::xau_usd(),
    ];

    // Method 1: Download with period string
    println!("--- Download with period ---\n");

    let data = download(&tickers, "1d").await?;

    for (ticker, rates) in &data {
        println!("{}: {} records", ticker.symbol(), rates.len());
    }

    // Method 2: Download with date range
    println!("\n--- Download with date range ---\n");

    let data = download_range(
        &tickers,
        datetime!(2025-1-2 00:00 UTC),
        datetime!(2025-1-3 23:59 UTC),
    )
    .await?;

    for (ticker, rates) in &data {
        if !rates.is_empty() {
            println!(
                "{}: {} records ({} to {})",
                ticker.symbol(),
                rates.len(),
                rates.first().unwrap().timestamp.format("%Y-%m-%d %H:%M"),
                rates.last().unwrap().timestamp.format("%Y-%m-%d %H:%M")
            );
        }
    }

    // Method 3: Export to CSV
    println!("\n--- CSV Export ---\n");

    // Convert to HashMap for easier CSV export
    let mut data_map: HashMap<String, Vec<_>> = HashMap::new();
    for (ticker, rates) in data {
        data_map.insert(ticker.symbol(), rates);
    }

    // Print CSV header
    println!("pair,timestamp,rate,bid,ask,spread");

    // Print first 5 rows for each pair
    for (symbol, rates) in &data_map {
        for rate in rates.iter().take(5) {
            println!(
                "{},{},{},{},{},{}",
                symbol,
                rate.timestamp.format("%Y-%m-%d %H:%M:%S"),
                rate.rate,
                rate.bid,
                rate.ask,
                rate.spread()
            );
        }
    }
    println!("... (truncated)");

    Ok(())
}
