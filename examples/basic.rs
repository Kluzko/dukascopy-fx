use chrono::{TimeZone, Utc};
use dukascopy_fx::{CurrencyPair, DukascopyFxService};

#[tokio::main]
async fn main() {
    env_logger::init();

    let pair = CurrencyPair {
        from: "USD".to_string(),
        to: "PLN".to_string(),
    };

    let timestamp = Utc.with_ymd_and_hms(2025, 01, 03, 14, 45, 0).unwrap();

    match DukascopyFxService::get_exchange_rate(&pair, timestamp).await {
        Ok(exchange) => {
            println!("Successfully fetched exchange rate: {:#?}", exchange);
        }
        Err(e) => {
            eprintln!("Error fetching exchange rate: {}", e);
        }
    }
}
