use chrono::{TimeZone, Utc};
use dukascopy_fx::{CurrencyPair, DukascopyFxService};

#[tokio::main]
async fn main() {
    env_logger::init();

    // Create currency pair using constructor
    let pair = CurrencyPair::new("USD", "PLN");

    // Or parse from string
    // let pair: CurrencyPair = "USD/PLN".parse().unwrap();

    let timestamp = Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap();

    match DukascopyFxService::get_exchange_rate(&pair, timestamp).await {
        Ok(exchange) => {
            println!("Successfully fetched exchange rate:");
            println!("  Pair: {}", exchange.pair);
            println!("  Rate: {}", exchange.rate);
            println!("  Bid: {}", exchange.bid);
            println!("  Ask: {}", exchange.ask);
            println!("  Spread: {}", exchange.spread());
            println!("  Timestamp: {}", exchange.timestamp);
        }
        Err(e) => {
            eprintln!("Error fetching exchange rate: {}", e);
        }
    }

    // Example with JPY pair (uses different price divisor)
    let jpy_pair = CurrencyPair::usd_jpy();
    match DukascopyFxService::get_exchange_rate(&jpy_pair, timestamp).await {
        Ok(exchange) => {
            println!("\nUSD/JPY exchange rate:");
            println!("  Rate: {}", exchange.rate);
            println!("  Bid: {}", exchange.bid);
            println!("  Ask: {}", exchange.ask);
        }
        Err(e) => {
            eprintln!("Error fetching USD/JPY rate: {}", e);
        }
    }

    // Example with Gold
    let gold_pair = CurrencyPair::xau_usd();
    match DukascopyFxService::get_exchange_rate(&gold_pair, timestamp).await {
        Ok(exchange) => {
            println!("\nXAU/USD (Gold) exchange rate:");
            println!("  Rate: {}", exchange.rate);
        }
        Err(e) => {
            eprintln!("Error fetching Gold rate: {}", e);
        }
    }
}
