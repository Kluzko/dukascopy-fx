//! Advanced usage examples for dukascopy-fx
//!
//! This example demonstrates:
//! - Fetching rates for multiple currency pairs
//! - Working with different instrument types (forex, JPY, metals)
//! - Using market hours utilities
//! - Error handling patterns
//! - Working with bid/ask spreads

use chrono::{Duration, TimeZone, Utc};
use dukascopy_fx::{
    get_market_status, is_market_open, is_weekend, CurrencyExchange, CurrencyPair, DukascopyError,
    DukascopyFxService, MarketStatus,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("=== Dukascopy FX Advanced Examples ===\n");

    // Example 1: Multiple currency pairs
    example_multiple_pairs().await?;

    // Example 2: Different instrument types
    example_instrument_types().await?;

    // Example 3: Market hours
    example_market_hours();

    // Example 4: Error handling
    example_error_handling().await;

    // Example 5: Spread analysis
    example_spread_analysis().await?;

    // Example 6: Time range fetching
    example_time_range().await?;

    Ok(())
}

/// Example 1: Fetching multiple currency pairs
async fn example_multiple_pairs() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 1: Multiple Currency Pairs ---");

    let pairs = vec![
        CurrencyPair::eur_usd(),
        CurrencyPair::gbp_usd(),
        CurrencyPair::usd_jpy(),
        CurrencyPair::new("USD", "CHF"),
        CurrencyPair::new("AUD", "USD"),
    ];

    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 30, 0).unwrap();

    println!("Fetching rates for {} at {}\n", timestamp, pairs.len());

    for pair in &pairs {
        match DukascopyFxService::get_exchange_rate(pair, timestamp).await {
            Ok(exchange) => {
                println!("  {}: {}", pair, exchange.rate);
            }
            Err(e) => {
                println!("  {}: Error - {}", pair, e);
            }
        }
    }

    println!();
    Ok(())
}

/// Example 2: Different instrument types with correct price scaling
async fn example_instrument_types() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 2: Different Instrument Types ---");

    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 30, 0).unwrap();

    // Standard forex (5 decimal places, divisor 100,000)
    let eur_usd = CurrencyPair::eur_usd();
    if let Ok(ex) = DukascopyFxService::get_exchange_rate(&eur_usd, timestamp).await {
        println!("  Standard Forex - {}: {} (5 decimals)", eur_usd, ex.rate);
    }

    // JPY pair (3 decimal places, divisor 1,000)
    let usd_jpy = CurrencyPair::usd_jpy();
    if let Ok(ex) = DukascopyFxService::get_exchange_rate(&usd_jpy, timestamp).await {
        println!("  JPY Pair - {}: {} (3 decimals)", usd_jpy, ex.rate);
    }

    // Gold (3 decimal places, divisor 1,000)
    let xau_usd = CurrencyPair::xau_usd();
    if let Ok(ex) = DukascopyFxService::get_exchange_rate(&xau_usd, timestamp).await {
        println!("  Gold - {}: {} (3 decimals)", xau_usd, ex.rate);
    }

    // Silver
    let xag_usd = CurrencyPair::xag_usd();
    if let Ok(ex) = DukascopyFxService::get_exchange_rate(&xag_usd, timestamp).await {
        println!("  Silver - {}: {} (3 decimals)", xag_usd, ex.rate);
    }

    println!();
    Ok(())
}

/// Example 3: Market hours utilities
fn example_market_hours() {
    println!("--- Example 3: Market Hours ---");

    // Check various timestamps
    let timestamps = vec![
        (
            "Monday 10:00",
            Utc.with_ymd_and_hms(2025, 1, 6, 10, 0, 0).unwrap(),
        ),
        (
            "Friday 20:00",
            Utc.with_ymd_and_hms(2025, 1, 3, 20, 0, 0).unwrap(),
        ),
        (
            "Friday 23:00",
            Utc.with_ymd_and_hms(2025, 1, 3, 23, 0, 0).unwrap(),
        ),
        (
            "Saturday 12:00",
            Utc.with_ymd_and_hms(2025, 1, 4, 12, 0, 0).unwrap(),
        ),
        (
            "Sunday 10:00",
            Utc.with_ymd_and_hms(2025, 1, 5, 10, 0, 0).unwrap(),
        ),
        (
            "Sunday 23:00",
            Utc.with_ymd_and_hms(2025, 1, 5, 23, 0, 0).unwrap(),
        ),
    ];

    for (name, ts) in timestamps {
        let is_open = is_market_open(ts);
        let weekend = is_weekend(ts);
        println!(
            "  {}: market_open={}, is_weekend={}",
            name, is_open, weekend
        );
    }

    // Get detailed market status
    let saturday = Utc.with_ymd_and_hms(2025, 1, 4, 12, 0, 0).unwrap();
    match get_market_status(saturday) {
        MarketStatus::Open => println!("\n  Saturday status: Open"),
        MarketStatus::Weekend { reopens_at } => {
            println!("\n  Saturday status: Closed, reopens at {}", reopens_at)
        }
        MarketStatus::Holiday { name, reopens_at } => {
            println!(
                "\n  Saturday status: Holiday {:?}, reopens at {}",
                name, reopens_at
            )
        }
    }

    println!();
}

/// Example 4: Error handling patterns
async fn example_error_handling() {
    println!("--- Example 4: Error Handling ---");

    let pair = CurrencyPair::eur_usd();

    // Try fetching from far future (no data)
    let future_date = Utc.with_ymd_and_hms(2030, 1, 1, 12, 0, 0).unwrap();
    match DukascopyFxService::get_exchange_rate(&pair, future_date).await {
        Ok(_) => println!("  Future date: Success (unexpected)"),
        Err(e) => {
            println!("  Future date error: {}", e);
            println!("    is_retryable: {}", e.is_retryable());
            println!("    is_not_found: {}", e.is_not_found());
        }
    }

    // Try invalid currency pair
    let invalid_pair = CurrencyPair::new("XX", "YY"); // Will be "XX" and "YY" (2 chars each)
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 0, 0).unwrap();
    match DukascopyFxService::get_exchange_rate(&invalid_pair, timestamp).await {
        Ok(_) => println!("  Invalid pair: Success (unexpected)"),
        Err(e) => {
            println!("  Invalid pair error: {}", e);
            println!("    is_validation_error: {}", e.is_validation_error());
        }
    }

    // Retry pattern for retryable errors
    async fn fetch_with_retry(
        pair: &CurrencyPair,
        timestamp: chrono::DateTime<Utc>,
        max_retries: u32,
    ) -> Result<CurrencyExchange, DukascopyError> {
        let mut last_error = None;

        for attempt in 0..max_retries {
            match DukascopyFxService::get_exchange_rate(pair, timestamp).await {
                Ok(exchange) => return Ok(exchange),
                Err(e) if e.is_retryable() => {
                    println!(
                        "    Attempt {} failed (retryable): {}, retrying...",
                        attempt + 1,
                        e
                    );
                    last_error = Some(e);
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(e) => return Err(e), // Non-retryable error
            }
        }

        Err(last_error.unwrap_or(DukascopyError::Unknown("Max retries exceeded".to_string())))
    }

    println!("\n  Retry pattern example:");
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 0, 0).unwrap();
    match fetch_with_retry(&pair, timestamp, 3).await {
        Ok(ex) => println!("    Success: {}", ex.rate),
        Err(e) => println!("    Failed after retries: {}", e),
    }

    println!();
}

/// Example 5: Spread analysis
async fn example_spread_analysis() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 5: Spread Analysis ---");

    let pairs = vec![
        CurrencyPair::eur_usd(),
        CurrencyPair::usd_jpy(),
        CurrencyPair::xau_usd(),
    ];

    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 30, 0).unwrap();

    println!(
        "  {:12} {:>12} {:>12} {:>12} {:>10}",
        "Pair", "Bid", "Ask", "Rate", "Spread"
    );
    println!("  {}", "-".repeat(62));

    for pair in &pairs {
        if let Ok(ex) = DukascopyFxService::get_exchange_rate(pair, timestamp).await {
            println!(
                "  {:12} {:>12} {:>12} {:>12} {:>10}",
                pair.to_string(),
                ex.bid,
                ex.ask,
                ex.rate,
                ex.spread()
            );
        }
    }

    println!();
    Ok(())
}

/// Example 6: Fetching rates over a time range
async fn example_time_range() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 6: Time Range Fetching ---");

    let pair = CurrencyPair::eur_usd();
    let start = Utc.with_ymd_and_hms(2025, 1, 3, 10, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 1, 3, 14, 0, 0).unwrap();
    let interval = Duration::hours(1);

    println!("  Fetching {} hourly rates from {} to {}", pair, start, end);

    let rates = DukascopyFxService::get_exchange_rates_range(&pair, start, end, interval).await?;

    println!("  Retrieved {} rates:\n", rates.len());
    for exchange in &rates {
        println!(
            "    {} - {}",
            exchange.timestamp.format("%H:%M"),
            exchange.rate
        );
    }

    // Calculate statistics
    if !rates.is_empty() {
        let rates_vec: Vec<f64> = rates
            .iter()
            .map(|e| e.rate.to_string().parse().unwrap_or(0.0))
            .collect();

        let min = rates_vec.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = rates_vec.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = rates_vec.iter().sum::<f64>() / rates_vec.len() as f64;

        println!("\n  Statistics:");
        println!("    Min: {:.5}", min);
        println!("    Max: {:.5}", max);
        println!("    Avg: {:.5}", avg);
        println!("    Range: {:.5}", max - min);
    }

    println!();
    Ok(())
}
