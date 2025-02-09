use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use std::matches;
use dukascopy_fx::{CurrencyPair, DukascopyFxService, DukascopyError};

#[tokio::test]
async fn test_get_exchange_rate() {
    let pair = CurrencyPair {
        from: "USD".to_string(),
        to: "PLN".to_string(),
    };

    let timestamp = Utc.with_ymd_and_hms(2025, 01, 03, 14, 45, 0).unwrap();

    let result = DukascopyFxService::get_exchange_rate(&pair, timestamp).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_get_exchange_rate_valid() {
    let pair = CurrencyPair {
        from: "EUR".to_string(),
        to: "USD".to_string(),
    };
    let timestamp = Utc.with_ymd_and_hms(2025, 2, 5, 14, 30, 0).unwrap();

    let result = DukascopyFxService::get_exchange_rate(&pair, timestamp).await;
    assert!(result.is_ok());
    let exchange = result.unwrap();
    assert_eq!(exchange.pair, pair);
    assert!(exchange.rate > Decimal::ZERO);
}

#[tokio::test]
async fn test_get_exchange_rate_invalid_currency_code() {
    let pair = CurrencyPair {
        from: "INVALID".to_string(),
        to: "PAIR".to_string(),
    };
    let timestamp = Utc.with_ymd_and_hms(2025, 2, 5, 14, 30, 0).unwrap();

    let result = DukascopyFxService::get_exchange_rate(&pair, timestamp).await;
    matches!(result, Err(DukascopyError::InvalidCurrencyCode));
}

#[tokio::test]
async fn test_get_last_tick_of_day_invalid() {
    let pair = CurrencyPair {
        from: "XXX".to_string(),
        to: "YYY".to_string(),
    };
    let date = Utc.with_ymd_and_hms(2025, 2, 7, 0, 0, 0).unwrap();

    let result = DukascopyFxService::get_last_tick_of_day(&pair, date).await;
    matches!(result, Err(DukascopyError::DataNotFound));
}
