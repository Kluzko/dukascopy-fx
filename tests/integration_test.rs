//! Integration tests for dukascopy-fx
//!
//! These tests hit the real Dukascopy API to verify:
//! - Data fetching works correctly
//! - Price divisors are applied correctly for different instruments
//! - Ticker API works as expected
//! - Weekend/market hours handling
//!
//! Set `LIVE_TESTS=1` to run them against live services.

use chrono::{Datelike, Duration, TimeZone, Timelike, Utc};
use dukascopy_fx::advanced::{ConversionMode, DukascopyClientBuilder, PairResolutionMode};
use dukascopy_fx::{CurrencyPair, DukascopyError, RateRequest, Ticker};
use serde::Deserialize;
use serial_test::serial;

#[derive(Debug, Deserialize)]
struct StooqDailyRow {
    #[serde(rename = "Date")]
    date: String,
    #[serde(rename = "Close")]
    close: f64,
}

async fn stooq_daily_close(symbol: &str, date: &str) -> Result<f64, Box<dyn std::error::Error>> {
    let url = format!("https://stooq.com/q/d/l/?s={}&i=d", symbol);
    let response = reqwest::get(&url).await?;
    if !response.status().is_success() {
        return Err(format!("Stooq request failed for {}: {}", symbol, response.status()).into());
    }

    let body = response.text().await?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(body.as_bytes());

    for row in reader.deserialize::<StooqDailyRow>() {
        let row = row?;
        if row.date == date {
            return Ok(row.close);
        }
    }

    Err(format!("No Stooq close found for symbol={} date={}", symbol, date).into())
}

fn relative_diff(left: f64, right: f64) -> f64 {
    if right == 0.0 {
        return f64::INFINITY;
    }
    (left - right).abs() / right.abs()
}

fn live_tests_enabled() -> bool {
    std::env::var("LIVE_TESTS")
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

macro_rules! require_live_tests {
    () => {
        if !live_tests_enabled() {
            eprintln!("Skipping live integration test. Set LIVE_TESTS=1 to enable.");
            return;
        }
    };
}

// ============================================================================
// Basic API Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_get_rate_usd_pln() {
    require_live_tests!();
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
#[serial]
async fn test_get_rate_eur_usd() {
    require_live_tests!();
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

#[tokio::test]
#[serial]
async fn test_get_rate_aapl_usd_supports_market_instrument_path() {
    require_live_tests!();
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();
    let result = dukascopy_fx::get_rate("AAPL", "USD", timestamp).await;

    match result {
        Ok(exchange) => {
            let rate: f64 = exchange.rate.try_into().unwrap();
            assert!(rate > 1.0, "AAPL/USD rate should be positive, got {}", rate);
        }
        Err(err) => {
            assert!(
                !err.is_validation_error(),
                "AAPL/USD should not fail validation path, got: {}",
                err
            );
        }
    }
}

#[tokio::test]
#[serial]
async fn test_get_rate_for_input_supports_explicit_pair() {
    require_live_tests!();
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();
    let result = dukascopy_fx::get_rate_for_input("EUR/USD", timestamp).await;

    assert!(
        result.is_ok(),
        "get_rate_for_input with pair failed: {:?}",
        result.err()
    );
}

#[tokio::test]
#[serial]
async fn test_get_rate_for_request_supports_single_symbol() {
    require_live_tests!();
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();
    let request = RateRequest::symbol("AAPL").unwrap();
    let result = dukascopy_fx::get_rate_for_request(&request, timestamp).await;

    match result {
        Ok(exchange) => {
            let rate: f64 = exchange.rate.try_into().unwrap();
            assert!(rate > 1.0, "AAPL/USD rate should be positive, got {}", rate);
        }
        Err(err) => {
            assert!(
                !matches!(
                    err,
                    DukascopyError::MissingDefaultQuoteCurrency
                        | DukascopyError::PairResolutionDisabled
                ),
                "global request API should support single-symbol resolution, got: {}",
                err
            );
            assert!(
                !err.is_validation_error(),
                "AAPL request should not fail validation path, got: {}",
                err
            );
        }
    }
}

#[tokio::test]
#[serial]
async fn test_client_default_quote_symbol_request_matches_explicit_pair() {
    require_live_tests!();
    let client = DukascopyClientBuilder::new()
        .default_quote_currency("PLN")
        .pair_resolution_mode(PairResolutionMode::ExplicitOrDefaultQuote)
        .conversion_mode(ConversionMode::DirectOnly)
        .build();

    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();
    let symbol_rate = client.get_exchange_rate_for_symbol("USD", timestamp).await;
    let explicit_rate = client
        .get_exchange_rate(&CurrencyPair::new("USD", "PLN"), timestamp)
        .await;

    assert!(
        symbol_rate.is_ok(),
        "symbol request failed: {:?}",
        symbol_rate
    );
    assert!(
        explicit_rate.is_ok(),
        "explicit pair request failed: {:?}",
        explicit_rate
    );

    let symbol_rate = symbol_rate.unwrap();
    let explicit_rate = explicit_rate.unwrap();
    assert_eq!(symbol_rate.rate, explicit_rate.rate);
}

// ============================================================================
// Price Divisor Tests (Critical for Dukascopy data)
// ============================================================================

#[tokio::test]
#[serial]
async fn test_jpy_pair_correct_divisor() {
    require_live_tests!();
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
#[serial]
async fn test_gold_correct_divisor() {
    require_live_tests!();
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
#[serial]
async fn test_silver_correct_divisor() {
    require_live_tests!();
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
#[serial]
async fn test_standard_pair_correct_divisor() {
    require_live_tests!();
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

#[tokio::test]
#[serial]
async fn test_cross_source_metals_close_is_reasonably_close_to_stooq() {
    require_live_tests!();
    let date = "2025-01-10";

    let xau_ts = Utc.with_ymd_and_hms(2025, 1, 10, 21, 55, 0).unwrap();
    let xag_ts = Utc.with_ymd_and_hms(2025, 1, 10, 21, 55, 0).unwrap();

    let xau = dukascopy_fx::get_rate("XAU", "USD", xau_ts)
        .await
        .expect("Failed to fetch XAU/USD from Dukascopy");
    let xag = dukascopy_fx::get_rate("XAG", "USD", xag_ts)
        .await
        .expect("Failed to fetch XAG/USD from Dukascopy");

    let xau_ref = stooq_daily_close("xauusd", date)
        .await
        .expect("Failed to fetch XAUUSD close from Stooq");
    let xag_ref = stooq_daily_close("xagusd", date)
        .await
        .expect("Failed to fetch XAGUSD close from Stooq");

    let xau_rate: f64 = xau.rate.try_into().unwrap();
    let xag_rate: f64 = xag.rate.try_into().unwrap();
    let xau_diff = relative_diff(xau_rate, xau_ref);
    let xag_diff = relative_diff(xag_rate, xag_ref);

    assert!(
        xau_diff <= 0.05,
        "XAU/USD differs too much vs Stooq: dukascopy={}, stooq={}, rel_diff={:.4}",
        xau_rate,
        xau_ref,
        xau_diff
    );
    assert!(
        xag_diff <= 0.05,
        "XAG/USD differs too much vs Stooq: dukascopy={}, stooq={}, rel_diff={:.4}",
        xag_rate,
        xag_ref,
        xag_diff
    );
}

#[tokio::test]
#[serial]
async fn test_cross_source_indices_close_is_reasonably_close_to_stooq() {
    require_live_tests!();
    let date = "2025-01-10";

    let usa500_ts = Utc.with_ymd_and_hms(2025, 1, 10, 21, 0, 0).unwrap();
    let deuidx_ts = Utc.with_ymd_and_hms(2025, 1, 10, 15, 30, 0).unwrap();

    let usa500 = dukascopy_fx::get_rate("USA500IDX", "USD", usa500_ts)
        .await
        .expect("Failed to fetch USA500IDX/USD from Dukascopy");
    let deuidx = dukascopy_fx::get_rate("DEUIDX", "EUR", deuidx_ts)
        .await
        .expect("Failed to fetch DEUIDX/EUR from Dukascopy");

    let usa500_ref = stooq_daily_close("%5Espx", date)
        .await
        .expect("Failed to fetch ^SPX close from Stooq");
    let deuidx_ref = stooq_daily_close("%5Edax", date)
        .await
        .expect("Failed to fetch ^DAX close from Stooq");

    let usa500_rate: f64 = usa500.rate.try_into().unwrap();
    let deuidx_rate: f64 = deuidx.rate.try_into().unwrap();

    let usa500_diff = relative_diff(usa500_rate, usa500_ref);
    let deuidx_diff = relative_diff(deuidx_rate, deuidx_ref);

    assert!(
        usa500_diff <= 0.08,
        "USA500IDX/USD differs too much vs ^SPX: dukascopy={}, stooq={}, rel_diff={:.4}",
        usa500_rate,
        usa500_ref,
        usa500_diff
    );
    assert!(
        deuidx_diff <= 0.08,
        "DEUIDX/EUR differs too much vs ^DAX: dukascopy={}, stooq={}, rel_diff={:.4}",
        deuidx_rate,
        deuidx_ref,
        deuidx_diff
    );
}

#[tokio::test]
#[serial]
async fn test_cross_source_us_stock_close_is_reasonably_close_to_stooq() {
    require_live_tests!();
    let date = "2025-01-10";
    let ts = Utc.with_ymd_and_hms(2025, 1, 10, 20, 59, 0).unwrap();

    let aapl = dukascopy_fx::get_rate("AAPLUS", "USD", ts)
        .await
        .expect("Failed to fetch AAPLUS/USD from Dukascopy");
    let aapl_ref = stooq_daily_close("aapl.us", date)
        .await
        .expect("Failed to fetch AAPL.US close from Stooq");

    let aapl_rate: f64 = aapl.rate.try_into().unwrap();
    let aapl_diff = relative_diff(aapl_rate, aapl_ref);

    assert!(
        aapl_diff <= 0.06,
        "AAPLUS/USD differs too much vs AAPL.US: dukascopy={}, stooq={}, rel_diff={:.4}",
        aapl_rate,
        aapl_ref,
        aapl_diff
    );
}

#[tokio::test]
#[serial]
async fn test_cross_source_additional_indices_close_is_reasonably_close_to_stooq() {
    require_live_tests!();
    let date = "2025-01-10";
    let usa_tech_ts = Utc.with_ymd_and_hms(2025, 1, 10, 20, 59, 0).unwrap();
    let hkg_ts = Utc.with_ymd_and_hms(2025, 1, 10, 16, 59, 0).unwrap();

    let usa_tech = dukascopy_fx::get_rate("USATECHIDX", "USD", usa_tech_ts)
        .await
        .expect("Failed to fetch USATECHIDX/USD from Dukascopy");
    let hkg = dukascopy_fx::get_rate("HKGIDX", "HKD", hkg_ts)
        .await
        .expect("Failed to fetch HKGIDX/HKD from Dukascopy");

    let usa_tech_ref = stooq_daily_close("%5Endx", date)
        .await
        .expect("Failed to fetch ^NDX close from Stooq");
    let hkg_ref = stooq_daily_close("%5Ehsi", date)
        .await
        .expect("Failed to fetch ^HSI close from Stooq");

    let usa_tech_rate: f64 = usa_tech.rate.try_into().unwrap();
    let hkg_rate: f64 = hkg.rate.try_into().unwrap();

    let usa_tech_diff = relative_diff(usa_tech_rate, usa_tech_ref);
    let hkg_diff = relative_diff(hkg_rate, hkg_ref);

    assert!(
        usa_tech_diff <= 0.08,
        "USATECHIDX/USD differs too much vs ^NDX: dukascopy={}, stooq={}, rel_diff={:.4}",
        usa_tech_rate,
        usa_tech_ref,
        usa_tech_diff
    );
    assert!(
        hkg_diff <= 0.08,
        "HKGIDX/HKD differs too much vs ^HSI: dukascopy={}, stooq={}, rel_diff={:.4}",
        hkg_rate,
        hkg_ref,
        hkg_diff
    );
}

// ============================================================================
// Ticker API Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_ticker_rate_at() {
    require_live_tests!();
    let ticker = Ticker::new("EUR", "USD");
    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = ticker.rate_at(timestamp).await;
    assert!(result.is_ok(), "Ticker.rate_at failed: {:?}", result.err());

    let exchange = result.unwrap();
    let rate: f64 = exchange.rate.try_into().unwrap();
    assert!(rate > 0.9 && rate < 1.5, "EUR/USD rate {} unexpected", rate);
}

#[tokio::test]
#[serial]
async fn test_ticker_convenience_constructors() {
    require_live_tests!();
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
#[serial]
async fn test_ticker_history_range() {
    require_live_tests!();
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
#[serial]
async fn test_ticker_parse() {
    require_live_tests!();
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
#[serial]
async fn test_currency_pair_parsing() {
    require_live_tests!();
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
#[serial]
async fn test_currency_pair_invalid() {
    require_live_tests!();
    assert!("E/USD".parse::<CurrencyPair>().is_err()); // Too short
    assert!("TOO_LONG_INSTRUMENT/USD".parse::<CurrencyPair>().is_err()); // Too long
    assert!("EUR".parse::<CurrencyPair>().is_err()); // Missing second currency
}

// ============================================================================
// Market Hours Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_market_hours() {
    require_live_tests!();
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
#[serial]
async fn test_weekend_data_returns_friday() {
    require_live_tests!();
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

#[tokio::test]
#[serial]
async fn test_friday_after_close_returns_last_tick() {
    require_live_tests!();
    // Friday after close should map to the last available Friday tick
    let friday_after_close = Utc.with_ymd_and_hms(2025, 1, 3, 22, 30, 0).unwrap();

    let result = dukascopy_fx::get_rate("EUR", "USD", friday_after_close).await;
    assert!(
        result.is_ok(),
        "Friday after close should return Friday last tick: {:?}",
        result.err()
    );

    let exchange = result.unwrap();
    assert_eq!(exchange.timestamp.weekday(), chrono::Weekday::Fri);
    assert_eq!(exchange.timestamp.hour(), 21);
}

// ============================================================================
// Range Query Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_get_rates_range() {
    require_live_tests!();
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

#[tokio::test]
#[serial]
async fn test_get_rates_range_rejects_non_positive_interval() {
    require_live_tests!();
    let end = Utc.with_ymd_and_hms(2025, 1, 3, 14, 0, 0).unwrap();
    let start = end - Duration::hours(3);

    let result = dukascopy_fx::get_rates_range("EUR", "USD", start, end, Duration::zero()).await;
    assert!(matches!(result, Err(DukascopyError::InvalidRequest(_))));
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
#[serial]
async fn test_invalid_currency_code() {
    require_live_tests!();
    let result = CurrencyPair::try_new("E", "USD");
    assert!(result.is_err());

    if let Err(e) = result {
        assert!(e.is_validation_error());
    }
}

#[tokio::test]
#[serial]
async fn test_future_date_no_data() {
    require_live_tests!();
    // Far future date - no data should exist
    let future = Utc.with_ymd_and_hms(2030, 1, 1, 12, 0, 0).unwrap();

    let result = dukascopy_fx::get_rate("EUR", "USD", future).await;
    assert!(result.is_err(), "Future date should return error");

    if let Err(e) = result {
        assert!(e.is_not_found(), "Error should be not_found, got: {}", e);
    }
}
