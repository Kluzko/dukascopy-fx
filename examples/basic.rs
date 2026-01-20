//! Basic usage example for dukascopy-fx
//!
//! Shows the yfinance-style API for fetching forex rates.

use dukascopy_fx::{datetime, Ticker};

#[tokio::main]
async fn main() -> dukascopy_fx::Result<()> {
    env_logger::init();

    println!("=== dukascopy-fx Basic Example ===\n");

    // ============================================================
    // Method 1: Ticker API (Recommended - yfinance style)
    // ============================================================

    println!("--- Ticker API ---\n");

    // Create a ticker
    let eur_usd = Ticker::new("EUR", "USD");

    // Get rate at specific time
    let rate = eur_usd.rate_at(datetime!(2025-01-03 14:30 UTC)).await?;
    println!("EUR/USD at 2025-01-03 14:30 UTC:");
    println!("  Rate: {}", rate.rate);
    println!("  Bid: {}, Ask: {}", rate.bid, rate.ask);
    println!("  Spread: {}", rate.spread());

    // Get historical data with period strings
    println!("\nLast day of hourly data:");
    let history = eur_usd.history("1d").await?;
    println!("  {} records fetched", history.len());
    if let Some(first) = history.first() {
        println!("  First: {} @ {}", first.rate, first.timestamp);
    }
    if let Some(last) = history.last() {
        println!("  Last:  {} @ {}", last.rate, last.timestamp);
    }

    // ============================================================
    // Method 2: Convenience Ticker Constructors
    // ============================================================

    println!("\n--- Different Instruments ---\n");

    // JPY pair (automatically uses correct 3 decimal precision)
    let jpy = Ticker::usd_jpy();
    let rate = jpy.rate_at(datetime!(2025-01-03 14:30 UTC)).await?;
    println!("USD/JPY: {} (3 decimal places)", rate.rate);

    // Gold
    let gold = Ticker::xau_usd();
    let rate = gold.rate_at(datetime!(2025-01-03 14:30 UTC)).await?;
    println!("XAU/USD: {} (Gold)", rate.rate);

    // Silver
    let silver = Ticker::xag_usd();
    let rate = silver.rate_at(datetime!(2025-01-03 14:30 UTC)).await?;
    println!("XAG/USD: {} (Silver)", rate.rate);

    // ============================================================
    // Method 3: Simple function API
    // ============================================================

    println!("\n--- Simple Function API ---\n");

    let rate = dukascopy_fx::get_rate("GBP", "USD", datetime!(2025-01-03 14:30 UTC)).await?;
    println!("GBP/USD: {}", rate.rate);

    Ok(())
}
