use chrono::{TimeZone, Utc};
use dukascopy_fx::{CurrencyPair, DukascopyFxService};

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
