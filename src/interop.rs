//! Interoperability helpers for analytics/dataframe pipelines.

use crate::models::CurrencyExchange;
use serde::{Deserialize, Serialize};

/// Flat row representation optimized for dataframe ingestion.
///
/// Decimal values are emitted as strings to avoid precision loss in serialization
/// pipelines that default to floating-point parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatExchangeRow {
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub timestamp_rfc3339: String,
    pub timestamp_ms: i64,
    pub rate: String,
    pub bid: String,
    pub ask: String,
    pub bid_volume: String,
    pub ask_volume: String,
}

/// Flattens a single exchange row using the provided symbol key.
pub fn flatten_row(symbol: &str, row: &CurrencyExchange) -> FlatExchangeRow {
    FlatExchangeRow {
        symbol: symbol.to_ascii_uppercase(),
        base: row.pair.from().to_string(),
        quote: row.pair.to().to_string(),
        timestamp_rfc3339: row.timestamp.to_rfc3339(),
        timestamp_ms: row.timestamp.timestamp_millis(),
        rate: row.rate.to_string(),
        bid: row.bid.to_string(),
        ask: row.ask.to_string(),
        bid_volume: row.bid_volume.to_string(),
        ask_volume: row.ask_volume.to_string(),
    }
}

/// Flattens a batch of exchange rows using the provided symbol key.
pub fn flatten_rows(symbol: &str, rows: &[CurrencyExchange]) -> Vec<FlatExchangeRow> {
    rows.iter().map(|row| flatten_row(symbol, row)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CurrencyPair;
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn test_flatten_row() {
        let row = CurrencyExchange {
            pair: CurrencyPair::try_new("EUR", "USD").unwrap(),
            rate: Decimal::from_str("1.10000").unwrap(),
            timestamp: Utc.with_ymd_and_hms(2025, 1, 3, 14, 30, 0).unwrap(),
            ask: Decimal::from_str("1.10010").unwrap(),
            bid: Decimal::from_str("1.09990").unwrap(),
            ask_volume: 12.5,
            bid_volume: 8.25,
        };

        let flat = flatten_row("eurusd", &row);
        assert_eq!(flat.symbol, "EURUSD");
        assert_eq!(flat.base, "EUR");
        assert_eq!(flat.quote, "USD");
        assert_eq!(flat.timestamp_ms, 1735914600000);
        assert_eq!(flat.rate, "1.10000");
        assert_eq!(flat.bid, "1.09990");
        assert_eq!(flat.ask, "1.10010");
    }

    #[test]
    fn test_flatten_rows_preserves_length() {
        let row = CurrencyExchange {
            pair: CurrencyPair::new("EUR", "USD"),
            rate: Decimal::from_str("1.10000").unwrap(),
            timestamp: Utc.with_ymd_and_hms(2025, 1, 3, 14, 30, 0).unwrap(),
            ask: Decimal::from_str("1.10010").unwrap(),
            bid: Decimal::from_str("1.09990").unwrap(),
            ask_volume: 1.0,
            bid_volume: 1.0,
        };

        let flat = flatten_rows("EURUSD", &[row.clone(), row]);
        assert_eq!(flat.len(), 2);
    }
}
