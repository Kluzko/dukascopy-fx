//! Batch download example for dukascopy-fx
//!
//! This example demonstrates how to efficiently download
//! historical data for multiple pairs and time ranges.

use chrono::{Duration, NaiveDate};
use dukascopy_fx::{CurrencyExchange, CurrencyPair, DukascopyError, DukascopyFxService};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("=== Batch Download Example ===\n");

    // Define pairs to download
    let pairs = vec![
        CurrencyPair::eur_usd(),
        CurrencyPair::gbp_usd(),
        CurrencyPair::usd_jpy(),
    ];

    // Define date range
    let start_date = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    let end_date = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();

    // Fetch hourly data for all pairs
    let results = batch_download(&pairs, start_date, end_date, Duration::hours(1)).await;

    // Print summary
    println!("\n=== Download Summary ===\n");
    for (pair, exchanges) in &results {
        match exchanges {
            Ok(data) => {
                println!("{}: {} records downloaded", pair, data.len());
                if let (Some(first), Some(last)) = (data.first(), data.last()) {
                    println!(
                        "  Range: {} to {}",
                        first.timestamp.format("%Y-%m-%d %H:%M"),
                        last.timestamp.format("%Y-%m-%d %H:%M")
                    );
                    println!("  First rate: {}, Last rate: {}", first.rate, last.rate);
                }
            }
            Err(e) => {
                println!("{}: Error - {}", pair, e);
            }
        }
        println!();
    }

    // Export to CSV format (demonstration)
    println!("=== CSV Export Example ===\n");
    export_csv(&results);

    Ok(())
}

/// Download data for multiple pairs over a date range
async fn batch_download(
    pairs: &[CurrencyPair],
    start_date: NaiveDate,
    end_date: NaiveDate,
    interval: Duration,
) -> HashMap<CurrencyPair, Result<Vec<CurrencyExchange>, DukascopyError>> {
    let mut results = HashMap::new();

    let start = start_date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end = end_date.and_hms_opt(23, 59, 59).unwrap().and_utc();

    for pair in pairs {
        println!("Downloading {}...", pair);

        let result = DukascopyFxService::get_exchange_rates_range(pair, start, end, interval).await;

        match &result {
            Ok(data) => println!("  Downloaded {} records", data.len()),
            Err(e) => println!("  Error: {}", e),
        }

        results.insert(pair.clone(), result);
    }

    results
}

/// Export results to CSV format (prints to stdout)
fn export_csv(results: &HashMap<CurrencyPair, Result<Vec<CurrencyExchange>, DukascopyError>>) {
    println!("pair,timestamp,rate,bid,ask,spread,bid_volume,ask_volume");

    for (pair, exchanges) in results {
        if let Ok(data) = exchanges {
            for ex in data {
                println!(
                    "{},{},{},{},{},{},{},{}",
                    pair,
                    ex.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    ex.rate,
                    ex.bid,
                    ex.ask,
                    ex.spread(),
                    ex.bid_volume,
                    ex.ask_volume
                );
            }
        }
    }
}
