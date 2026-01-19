use chrono::{TimeZone, Utc};
use dukascopy_fx::{CurrencyPair, DukascopyFxService};

#[tokio::test]
async fn test_get_exchange_rate() {
    let pair = CurrencyPair::new("USD", "PLN");

    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = DukascopyFxService::get_exchange_rate(&pair, timestamp).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_get_exchange_rate_jpy_pair() {
    let pair = CurrencyPair::usd_jpy();

    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    let result = DukascopyFxService::get_exchange_rate(&pair, timestamp).await;
    assert!(result.is_ok());

    let exchange = result.unwrap();
    // USD/JPY should be in reasonable range (100-200)
    let rate_f64: f64 = exchange.rate.try_into().unwrap();
    assert!(
        rate_f64 > 100.0 && rate_f64 < 200.0,
        "USD/JPY rate {} is out of expected range",
        rate_f64
    );
}

#[tokio::test]
async fn test_currency_pair_parsing() {
    let pair1: CurrencyPair = "EUR/USD".parse().unwrap();
    assert_eq!(pair1.from(), "EUR");
    assert_eq!(pair1.to(), "USD");

    let pair2: CurrencyPair = "GBPJPY".parse().unwrap();
    assert_eq!(pair2.from(), "GBP");
    assert_eq!(pair2.to(), "JPY");
}
