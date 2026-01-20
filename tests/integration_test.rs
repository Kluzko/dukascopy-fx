//! Integration tests for dukascopy-fx
//!
//! These tests hit the real Dukascopy API to verify:
//! - Data fetching works correctly
//! - Price divisors are applied correctly for different instruments
//! - Ticker API works as expected
//! - Weekend/market hours handling

use chrono::{Datelike, Duration, TimeZone, Utc};
use dukascopy_fx::{CurrencyPair, Ticker};

// ============================================================================
// Basic API Tests
// ============================================================================

#[tokio::test]
async fn test_get_rate_usd_pln() {
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = dukascopy_fx::get_rate("USD", "PLN", timestamp).await;
    assert!(
        result.is_ok(),
        "Failed to fetch USD/PLN: {:?}",
        result.err()
    );

    let exchange = result.unwrap();
    let rate: f64 = exchange.rate.try_into().unwrap();

    // USD/PLN should be in reasonable range (3-5 PLN per USD historically)
    assert!(
        rate > 3.0 && rate < 6.0,
        "USD/PLN rate {} is out of expected range (3-6)",
        rate
    );
}

#[tokio::test]
async fn test_get_rate_eur_usd() {
    let pair = CurrencyPair::new("EUR", "USD");
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = dukascopy_fx::get_rate_for_pair(&pair, timestamp).await;
    assert!(
        result.is_ok(),
        "Failed to fetch EUR/USD: {:?}",
        result.err()
    );

    let exchange = result.unwrap();
    let rate: f64 = exchange.rate.try_into().unwrap();

    // EUR/USD should be in reasonable range (0.9-1.3)
    assert!(
        rate > 0.9 && rate < 1.5,
        "EUR/USD rate {} is out of expected range (0.9-1.5)",
        rate
    );

    // Verify bid < ask (spread is positive)
    assert!(
        exchange.bid < exchange.ask,
        "Bid {} should be less than ask {}",
        exchange.bid,
        exchange.ask
    );
}

// ============================================================================
// Price Divisor Tests (Critical for Dukascopy data)
// ============================================================================

#[tokio::test]
async fn test_jpy_pair_correct_divisor() {
    // JPY pairs use divisor 1000 (3 decimal places)
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = dukascopy_fx::get_rate("USD", "JPY", timestamp).await;
    assert!(
        result.is_ok(),
        "Failed to fetch USD/JPY: {:?}",
        result.err()
    );

    let exchange = result.unwrap();
    let rate: f64 = exchange.rate.try_into().unwrap();

    // USD/JPY should be 100-200 range (not 100000-200000 which would indicate wrong divisor)
    assert!(
        rate > 100.0 && rate < 200.0,
        "USD/JPY rate {} is out of expected range. If > 100000, divisor is wrong!",
        rate
    );
}

#[tokio::test]
async fn test_gold_correct_divisor() {
    // Gold (XAU/USD) uses divisor 1000 (3 decimal places)
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = dukascopy_fx::get_rate("XAU", "USD", timestamp).await;
    assert!(
        result.is_ok(),
        "Failed to fetch XAU/USD: {:?}",
        result.err()
    );

    let exchange = result.unwrap();
    let rate: f64 = exchange.rate.try_into().unwrap();

    // Gold should be 1500-3000 USD/oz range (not millions which would indicate wrong divisor)
    assert!(
        rate > 1500.0 && rate < 3500.0,
        "XAU/USD rate {} is out of expected range (1500-3500). Check divisor!",
        rate
    );
}

#[tokio::test]
async fn test_silver_correct_divisor() {
    // Silver (XAG/USD) uses divisor 1000
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = dukascopy_fx::get_rate("XAG", "USD", timestamp).await;
    assert!(
        result.is_ok(),
        "Failed to fetch XAG/USD: {:?}",
        result.err()
    );

    let exchange = result.unwrap();
    let rate: f64 = exchange.rate.try_into().unwrap();

    // Silver should be 15-50 USD/oz range
    assert!(
        rate > 15.0 && rate < 60.0,
        "XAG/USD rate {} is out of expected range (15-60). Check divisor!",
        rate
    );
}

#[tokio::test]
async fn test_standard_pair_correct_divisor() {
    // Standard pairs (EUR/USD) use divisor 100000 (5 decimal places)
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = dukascopy_fx::get_rate("GBP", "USD", timestamp).await;
    assert!(
        result.is_ok(),
        "Failed to fetch GBP/USD: {:?}",
        result.err()
    );

    let exchange = result.unwrap();
    let rate: f64 = exchange.rate.try_into().unwrap();

    // GBP/USD should be 1.1-1.5 range (not 0.00001 which would indicate wrong divisor)
    assert!(
        rate > 1.0 && rate < 1.6,
        "GBP/USD rate {} is out of expected range (1.0-1.6). Check divisor!",
        rate
    );
}

// ============================================================================
// Ticker API Tests
// ============================================================================

#[tokio::test]
async fn test_ticker_rate_at() {
    let ticker = Ticker::new("EUR", "USD");
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = ticker.rate_at(timestamp).await;
    assert!(result.is_ok(), "Ticker.rate_at failed: {:?}", result.err());

    let exchange = result.unwrap();
    let rate: f64 = exchange.rate.try_into().unwrap();
    assert!(rate > 0.9 && rate < 1.5, "EUR/USD rate {} unexpected", rate);
}

#[tokio::test]
async fn test_ticker_convenience_constructors() {
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    // Test EUR/USD convenience constructor
    let ticker = Ticker::eur_usd();
    let result = ticker.rate_at(timestamp).await;
    assert!(result.is_ok(), "Ticker::eur_usd() failed");

    // Test USD/JPY convenience constructor
    let ticker = Ticker::usd_jpy();
    let result = ticker.rate_at(timestamp).await;
    assert!(result.is_ok(), "Ticker::usd_jpy() failed");

    // Test XAU/USD convenience constructor
    let ticker = Ticker::xau_usd();
    let result = ticker.rate_at(timestamp).await;
    assert!(result.is_ok(), "Ticker::xau_usd() failed");
}

#[tokio::test]
async fn test_ticker_history_range() {
    let ticker = Ticker::new("EUR", "USD");
    let end = Utc.with_ymd_and_hms(2025, 1, 3, 14, 0, 0).unwrap();
    let start = end - Duration::hours(5);

    let result = ticker.history_range(start, end).await;
    assert!(result.is_ok(), "history_range failed: {:?}", result.err());

    let history = result.unwrap();
    // Should have approximately 5-6 hourly records
    assert!(
        history.len() >= 4 && history.len() <= 7,
        "Expected 4-7 records, got {}",
        history.len()
    );

    // Verify chronological order
    for i in 1..history.len() {
        assert!(
            history[i].timestamp >= history[i - 1].timestamp,
            "Records not in chronological order"
        );
    }
}

#[tokio::test]
async fn test_ticker_parse() {
    let ticker: Ticker = "EUR/USD".parse().unwrap();
    assert_eq!(ticker.symbol(), "EURUSD");

    let ticker: Ticker = "USDJPY".parse().unwrap();
    assert_eq!(ticker.symbol(), "USDJPY");

    let ticker = Ticker::parse("GBP/JPY").unwrap();
    assert_eq!(ticker.symbol(), "GBPJPY");
}

// ============================================================================
// Currency Pair Tests
// ============================================================================

#[tokio::test]
async fn test_currency_pair_parsing() {
    let pair1: CurrencyPair = "EUR/USD".parse().unwrap();
    assert_eq!(pair1.from(), "EUR");
    assert_eq!(pair1.to(), "USD");

    let pair2: CurrencyPair = "GBPJPY".parse().unwrap();
    assert_eq!(pair2.from(), "GBP");
    assert_eq!(pair2.to(), "JPY");

    // Case insensitive
    let pair3: CurrencyPair = "eur/usd".parse().unwrap();
    assert_eq!(pair3.from(), "EUR");
}

#[tokio::test]
async fn test_currency_pair_invalid() {
    assert!("EU/USD".parse::<CurrencyPair>().is_err()); // Too short
    assert!("EURO/USD".parse::<CurrencyPair>().is_err()); // Too long
    assert!("EUR".parse::<CurrencyPair>().is_err()); // Missing second currency
}

// ============================================================================
// Market Hours Tests
// ============================================================================

#[tokio::test]
async fn test_market_hours() {
    // Saturday - market closed
    let saturday = Utc.with_ymd_and_hms(2025, 1, 4, 12, 0, 0).unwrap();
    assert!(dukascopy_fx::is_weekend(saturday));
    assert!(!dukascopy_fx::is_market_open(saturday));

    // Monday - market open
    let monday = Utc.with_ymd_and_hms(2025, 1, 6, 12, 0, 0).unwrap();
    assert!(!dukascopy_fx::is_weekend(monday));
    assert!(dukascopy_fx::is_market_open(monday));

    // Friday before close (21:00 UTC)
    let friday_before = Utc.with_ymd_and_hms(2025, 1, 3, 20, 0, 0).unwrap();
    assert!(dukascopy_fx::is_market_open(friday_before));

    // Friday after close (22:00 UTC)
    let friday_after = Utc.with_ymd_and_hms(2025, 1, 3, 22, 0, 0).unwrap();
    assert!(!dukascopy_fx::is_market_open(friday_after));
}

#[tokio::test]
async fn test_weekend_data_returns_friday() {
    // Request data for Saturday - should return Friday's last data
    let saturday = Utc.with_ymd_and_hms(2025, 1, 4, 12, 0, 0).unwrap();

    let result = dukascopy_fx::get_rate("EUR", "USD", saturday).await;
    assert!(
        result.is_ok(),
        "Weekend request should work: {:?}",
        result.err()
    );

    let exchange = result.unwrap();
    // The timestamp should be from Friday, not Saturday
    assert_eq!(
        exchange.timestamp.weekday(),
        chrono::Weekday::Fri,
        "Weekend data should return Friday timestamp, got {:?}",
        exchange.timestamp.weekday()
    );
}

// ============================================================================
// Range Query Tests
// ============================================================================

#[tokio::test]
async fn test_get_rates_range() {
    let end = Utc.with_ymd_and_hms(2025, 1, 3, 14, 0, 0).unwrap();
    let start = end - Duration::hours(3);

    let result = dukascopy_fx::get_rates_range("EUR", "USD", start, end, Duration::hours(1)).await;
    assert!(result.is_ok(), "get_rates_range failed: {:?}", result.err());

    let rates = result.unwrap();
    assert!(
        rates.len() >= 3,
        "Expected at least 3 rates, got {}",
        rates.len()
    );

    // All rates should be reasonable EUR/USD values
    for rate in &rates {
        let r: f64 = rate.rate.try_into().unwrap();
        assert!(r > 0.9 && r < 1.5, "EUR/USD rate {} out of range", r);
    }
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_invalid_currency_code() {
    let result = CurrencyPair::try_new("EU", "USD");
    assert!(result.is_err());

    if let Err(e) = result {
        assert!(e.is_validation_error());
    }
}

#[tokio::test]
async fn test_future_date_no_data() {
    // Far future date - no data should exist
    let future = Utc.with_ymd_and_hms(2030, 1, 1, 12, 0, 0).unwrap();

    let result = dukascopy_fx::get_rate("EUR", "USD", future).await;
    assert!(result.is_err(), "Future date should return error");

    if let Err(e) = result {
        assert!(e.is_not_found(), "Error should be not_found, got: {}", e);
    }
}
