# `dukascopy-fx`

A Rust library for fetching **historical forex (currency exchange) data** from **Dukascopy's** tick data API. This library provides a simple and efficient way to retrieve exchange rates, handle weekends, and cache data for improved performance.

This library was created because I couldn't find a free API that provides historical forex data with minute- or tick-level precision.
Dukascopy's API is free and offers high-precision tick data for a wide range of currency pairs.

## Features

- **Fetch Historical Forex Data**: Retrieve tick data for specific currency pairs and timestamps.
- **Weekend Handling**: Automatically fetches the last available tick from Friday for weekend timestamps.
- **Caching**: Implements an LRU cache to reduce redundant API requests.
- **Error Handling**: Provides detailed error messages for invalid data, HTTP errors, and more.
- **Customizable**: Supports custom cache sizes, logging, and HTTP clients.

---

## Usage

### Fetching Exchange Rates

```rust
use dukascopy_rs::{DukascopyForexService, CurrencyPair};
use chrono::{Utc, TimeZone};

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::init();

    // Define the currency pair (e.g., USD/PLN)
    let pair = CurrencyPair {
        from: "USD".to_string(),
        to: "PLN".to_string(),
    };

    // Define the timestamp for which you want the exchange rate
    let timestamp = Utc.with_ymd_and_hms(2025, 01, 03, 14, 45, 0).unwrap();

    // Fetch the exchange rate
    match DukascopyForexService::get_exchange_rate(&pair, timestamp).await {
        Ok(exchange) => {
            println!("Successfully fetched exchange rate: {:#?}", exchange);
        }
        Err(e) => {
            eprintln!("Error fetching exchange rate: {}", e);
        }
    }
}
```
