use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Represents a currency pair (e.g., USD/PLN).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrencyPair {
    pub from: String,
    pub to: String,
}

/// Represents a currency exchange rate at a specific timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyExchange {
    pub pair: CurrencyPair,
    pub rate: Decimal,
    pub timestamp: DateTime<Utc>,
    pub bid_volume: f32,
    pub ask_volume: f32,
}
