//! Advanced usage examples for dukascopy-fx
//!
//! Demonstrates:
//! - Batch downloads
//! - Time utilities
//! - Market hours
//! - Error handling
//! - Historical analysis

use dukascopy_fx::{
    datetime, download, get_market_status, is_market_open, is_weekend,
    time::{days_ago, weeks_ago, Duration},
    DukascopyError, MarketStatus, Ticker,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("=== dukascopy-fx Advanced Examples ===\n");

    example_batch_download().await?;
    example_time_utilities().await?;
    example_market_hours();
    example_error_handling().await;
    example_historical_analysis().await?;

    Ok(())
}

/// Example 1: Batch download multiple tickers
async fn example_batch_download() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 1: Batch Download ---\n");

    let tickers = vec![
        Ticker::eur_usd(),
        Ticker::gbp_usd(),
        Ticker::usd_jpy(),
        Ticker::xau_usd(),
    ];

    // Download last 3 days of data for all tickers
    let data = download(&tickers, "3d").await?;

    println!("Downloaded data for {} tickers:\n", data.len());
    for (ticker, rates) in &data {
        if !rates.is_empty() {
            println!("  {}: {} records", ticker.symbol(), rates.len());
            println!(
                "    Range: {} to {}",
                rates.first().unwrap().timestamp.format("%Y-%m-%d %H:%M"),
                rates.last().unwrap().timestamp.format("%Y-%m-%d %H:%M")
            );
        }
    }

    println!();
    Ok(())
}

/// Example 2: Using time utilities
async fn example_time_utilities() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 2: Time Utilities ---\n");

    let ticker = Ticker::new("EUR", "USD");

    // Rate from 24 hours ago
    let rate = ticker.rate_at(days_ago(1)).await?;
    println!("EUR/USD 24 hours ago: {}", rate.rate);

    // Custom date range
    let history = ticker.history_range(weeks_ago(1), days_ago(1)).await?;
    println!("Last week (excluding today): {} records", history.len());

    // Using datetime! macro
    let rate = ticker.rate_at(datetime!(2025-01-03 10:00 UTC)).await?;
    println!("At 2025-01-03 10:00 UTC: {}", rate.rate);

    // Using period with custom interval
    let ticker_30min = Ticker::new("EUR", "USD").interval(Duration::minutes(30));
    let history = ticker_30min.history("1d").await?;
    println!("30-min intervals for 1 day: {} records", history.len());

    println!();
    Ok(())
}

/// Example 3: Market hours utilities
fn example_market_hours() {
    println!("--- Example 3: Market Hours ---\n");

    let timestamps = [
        ("Monday 10:00", datetime!(2025-01-06 10:00 UTC)),
        ("Friday 20:00", datetime!(2025-01-03 20:00 UTC)),
        ("Friday 23:00", datetime!(2025-01-03 23:00 UTC)),
        ("Saturday 12:00", datetime!(2025-01-04 12:00 UTC)),
        ("Sunday 10:00", datetime!(2025-01-05 10:00 UTC)),
        ("Sunday 23:00", datetime!(2025-01-05 23:00 UTC)),
    ];

    for (name, ts) in timestamps {
        let open = is_market_open(ts);
        let weekend = is_weekend(ts);
        println!("  {}: open={}, weekend={}", name, open, weekend);
    }

    // Detailed status
    let saturday = datetime!(2025-01-04 12:00 UTC);
    match get_market_status(saturday) {
        MarketStatus::Open => println!("\n  Saturday: Open"),
        MarketStatus::Weekend { reopens_at } => {
            println!("\n  Saturday: Closed, reopens {}", reopens_at)
        }
        MarketStatus::Holiday { name, reopens_at } => {
            println!("\n  Saturday: Holiday {:?}, reopens {}", name, reopens_at)
        }
    }

    println!();
}

/// Example 4: Error handling
async fn example_error_handling() {
    println!("--- Example 4: Error Handling ---\n");

    // Future date (no data)
    let ticker = Ticker::new("EUR", "USD");
    match ticker.rate_at(datetime!(2030-01-01 12:00 UTC)).await {
        Ok(_) => println!("  Future: Success (unexpected)"),
        Err(e) => {
            println!("  Future date error: {}", e);
            println!("    is_retryable: {}", e.is_retryable());
            println!("    is_not_found: {}", e.is_not_found());
        }
    }

    // Retry pattern
    async fn fetch_with_retry(
        ticker: &Ticker,
        timestamp: chrono::DateTime<chrono::Utc>,
        max_retries: u32,
    ) -> Result<dukascopy_fx::CurrencyExchange, DukascopyError> {
        let mut last_error = None;

        for attempt in 0..max_retries {
            match ticker.rate_at(timestamp).await {
                Ok(rate) => return Ok(rate),
                Err(e) if e.is_retryable() => {
                    println!("    Attempt {} failed (retryable)", attempt + 1);
                    last_error = Some(e);
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_error.unwrap_or(DukascopyError::Unknown("Max retries".into())))
    }

    println!("\n  Retry pattern:");
    match fetch_with_retry(&ticker, datetime!(2025-01-03 14:00 UTC), 3).await {
        Ok(rate) => println!("    Success: {}", rate.rate),
        Err(e) => println!("    Failed: {}", e),
    }

    println!();
}

/// Example 5: Historical analysis
async fn example_historical_analysis() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 5: Historical Analysis ---\n");

    let ticker = Ticker::new("EUR", "USD");
    let history = ticker.history("1w").await?;

    if history.is_empty() {
        println!("  No data available");
        return Ok(());
    }

    // Calculate statistics
    let rates: Vec<f64> = history
        .iter()
        .filter_map(|r| r.rate.to_string().parse().ok())
        .collect();

    let min = rates.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = rates.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let avg = rates.iter().sum::<f64>() / rates.len() as f64;

    println!("  EUR/USD Last Week Statistics:");
    println!("    Records: {}", rates.len());
    println!("    Min: {:.5}", min);
    println!("    Max: {:.5}", max);
    println!("    Avg: {:.5}", avg);
    println!("    Range: {:.5}", max - min);

    // Find largest move
    let mut max_move = 0.0f64;
    for i in 1..rates.len() {
        let move_size = (rates[i] - rates[i - 1]).abs();
        if move_size > max_move {
            max_move = move_size;
        }
    }
    println!("    Largest hourly move: {:.5}", max_move);

    println!();
    Ok(())
}
