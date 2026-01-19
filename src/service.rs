//! Main service for fetching forex exchange rates from Dukascopy.

use crate::client::DukascopyClient;
use crate::error::DukascopyError;
use crate::instrument::HasInstrumentConfig;
use crate::market::{is_weekend, last_available_tick_time};
use crate::models::{CurrencyExchange, CurrencyPair};
use crate::parser::{DukascopyParser, ParsedTick, TICK_SIZE_BYTES};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

/// Number of decimal places for rate rounding
const RATE_DECIMAL_PLACES: u32 = 4;

/// Service for fetching forex exchange rates from Dukascopy.
///
/// # Examples
///
/// ```no_run
/// use dukascopy_fx::{DukascopyFxService, CurrencyPair};
/// use chrono::{Utc, TimeZone};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pair = CurrencyPair::new("EUR", "USD");
/// let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 0).unwrap();
///
/// let exchange = DukascopyFxService::get_exchange_rate(&pair, timestamp).await?;
/// println!("Rate: {}", exchange.rate);
/// # Ok(())
/// # }
/// ```
pub struct DukascopyFxService;

impl DukascopyFxService {
    /// Fetches the exchange rate for a currency pair at a specific timestamp.
    ///
    /// # Arguments
    /// * `pair` - The currency pair to fetch
    /// * `timestamp` - The timestamp for the rate
    ///
    /// # Returns
    /// The exchange rate information including bid/ask prices
    ///
    /// # Errors
    /// - `InvalidCurrencyCode` - Currency codes are invalid
    /// - `DataNotFound` - No data available for the timestamp
    /// - `HttpError` - Network errors
    ///
    /// # Weekend Handling
    /// If the timestamp falls on a weekend, the last available tick from Friday
    /// before market close is returned automatically.
    pub async fn get_exchange_rate(
        pair: &CurrencyPair,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        // Validate currency codes
        Self::validate_pair(pair)?;

        // Handle weekend timestamps
        let effective_timestamp = if is_weekend(timestamp) {
            last_available_tick_time(timestamp)
        } else {
            timestamp
        };

        // Build URL and fetch data
        let url = Self::build_url(pair, effective_timestamp);
        let decompressed_data = DukascopyClient::get_cached_data(&url).await?;

        // Validate data before parsing
        DukascopyParser::validate_decompressed_data(&decompressed_data)?;

        // Find the closest tick to the requested time
        let target_ms = Self::timestamp_to_ms_from_hour(effective_timestamp);
        let config = pair.instrument_config();

        let closest_tick = Self::find_closest_tick(&decompressed_data, target_ms, config)?;

        // Build the response
        Self::build_exchange_response(pair, effective_timestamp, closest_tick)
    }

    /// Fetches exchange rates for a time range.
    ///
    /// # Arguments
    /// * `pair` - The currency pair to fetch
    /// * `start` - Start timestamp
    /// * `end` - End timestamp
    /// * `interval` - Time interval between samples
    ///
    /// # Returns
    /// Vector of exchange rates at the specified intervals
    pub async fn get_exchange_rates_range(
        pair: &CurrencyPair,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        interval: Duration,
    ) -> Result<Vec<CurrencyExchange>, DukascopyError> {
        Self::validate_pair(pair)?;

        if start >= end {
            return Err(DukascopyError::InvalidRequest(
                "Start time must be before end time".to_string(),
            ));
        }

        let mut results = Vec::new();
        let mut current = start;

        while current <= end {
            match Self::get_exchange_rate(pair, current).await {
                Ok(exchange) => results.push(exchange),
                Err(DukascopyError::DataNotFound) => {
                    // Skip timestamps without data
                }
                Err(e) => return Err(e),
            }
            current += interval;
        }

        Ok(results)
    }

    /// Gets the last tick of a specific hour.
    ///
    /// Useful for getting end-of-hour rates.
    pub async fn get_last_tick_of_hour(
        pair: &CurrencyPair,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        Self::validate_pair(pair)?;

        let hour_start = timestamp
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .ok_or_else(|| DukascopyError::InvalidRequest("Invalid timestamp".to_string()))?;

        let url = Self::build_url(pair, hour_start);
        let decompressed_data = DukascopyClient::get_cached_data(&url).await?;

        DukascopyParser::validate_decompressed_data(&decompressed_data)?;

        let config = pair.instrument_config();

        // Get the last tick
        let last_chunk = decompressed_data
            .chunks_exact(TICK_SIZE_BYTES)
            .last()
            .ok_or(DukascopyError::DataNotFound)?;

        let tick = DukascopyParser::parse_tick_with_config(last_chunk, config)?;

        Self::build_exchange_response(pair, hour_start, tick)
    }

    // ==================== Private Helper Methods ====================

    /// Validates the currency pair.
    fn validate_pair(pair: &CurrencyPair) -> Result<(), DukascopyError> {
        if pair.from().len() != 3 {
            return Err(DukascopyError::InvalidCurrencyCode {
                code: pair.from().to_string(),
                reason: "Currency code must be exactly 3 characters".to_string(),
            });
        }
        if pair.to().len() != 3 {
            return Err(DukascopyError::InvalidCurrencyCode {
                code: pair.to().to_string(),
                reason: "Currency code must be exactly 3 characters".to_string(),
            });
        }
        Ok(())
    }

    /// Builds the Dukascopy API URL for tick data.
    fn build_url(pair: &CurrencyPair, timestamp: DateTime<Utc>) -> String {
        DukascopyClient::build_url(
            &pair.as_symbol(),
            timestamp.year(),
            timestamp.month(),
            timestamp.day(),
            timestamp.hour(),
        )
    }

    /// Converts timestamp to milliseconds from the start of the hour.
    fn timestamp_to_ms_from_hour(timestamp: DateTime<Utc>) -> u32 {
        timestamp.minute() * 60_000 + timestamp.second() * 1_000
    }

    /// Finds the tick closest to the target milliseconds.
    fn find_closest_tick(
        data: &[u8],
        target_ms: u32,
        config: crate::instrument::InstrumentConfig,
    ) -> Result<ParsedTick, DukascopyError> {
        let mut closest_tick: Option<ParsedTick> = None;
        let mut min_diff = u32::MAX;

        for chunk in data.chunks_exact(TICK_SIZE_BYTES) {
            let tick = DukascopyParser::parse_tick_with_config(chunk, config)?;
            let diff = tick.ms_from_hour.abs_diff(target_ms);

            if diff < min_diff {
                min_diff = diff;
                closest_tick = Some(tick);

                // Exact match - exit early
                if diff == 0 {
                    break;
                }
            }
        }

        closest_tick.ok_or(DukascopyError::DataNotFound)
    }

    /// Builds the CurrencyExchange response from a parsed tick.
    fn build_exchange_response(
        pair: &CurrencyPair,
        base_timestamp: DateTime<Utc>,
        tick: ParsedTick,
    ) -> Result<CurrencyExchange, DukascopyError> {
        // Calculate mid price
        let mid_price = tick.mid_price();

        // Convert to Decimal with proper rounding
        let rate = Decimal::from_f64(mid_price)
            .ok_or_else(|| DukascopyError::Unknown("Invalid price conversion".to_string()))?;
        let rate =
            rate.round_dp_with_strategy(RATE_DECIMAL_PLACES, RoundingStrategy::MidpointNearestEven);

        let ask = Decimal::from_f64(tick.ask)
            .ok_or_else(|| DukascopyError::Unknown("Invalid ask price conversion".to_string()))?;
        let ask = ask.round_dp_with_strategy(
            RATE_DECIMAL_PLACES + 1,
            RoundingStrategy::MidpointNearestEven,
        );

        let bid = Decimal::from_f64(tick.bid)
            .ok_or_else(|| DukascopyError::Unknown("Invalid bid price conversion".to_string()))?;
        let bid = bid.round_dp_with_strategy(
            RATE_DECIMAL_PLACES + 1,
            RoundingStrategy::MidpointNearestEven,
        );

        // Calculate actual tick timestamp
        let tick_time = base_timestamp
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .ok_or_else(|| DukascopyError::Unknown("Invalid timestamp".to_string()))?
            + Duration::milliseconds(tick.ms_from_hour as i64);

        Ok(CurrencyExchange {
            pair: pair.clone(),
            rate,
            timestamp: tick_time,
            ask,
            bid,
            ask_volume: tick.ask_volume,
            bid_volume: tick.bid_volume,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    mod validate_pair {
        use super::*;

        #[test]
        fn test_valid_pair() {
            let pair = CurrencyPair::new("EUR", "USD");
            assert!(DukascopyFxService::validate_pair(&pair).is_ok());
        }

        #[test]
        fn test_invalid_from_too_short() {
            let pair = CurrencyPair::new("EU", "USD");
            let result = DukascopyFxService::validate_pair(&pair);
            assert!(result.is_err());
        }

        #[test]
        fn test_invalid_to_too_short() {
            let pair = CurrencyPair::new("EUR", "US");
            let result = DukascopyFxService::validate_pair(&pair);
            assert!(result.is_err());
        }
    }

    mod build_url {
        use super::*;

        #[test]
        fn test_standard_url() {
            let pair = CurrencyPair::new("EUR", "USD");
            let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
            let url = DukascopyFxService::build_url(&pair, timestamp);

            assert!(url.contains("EURUSD"));
            assert!(url.contains("2024"));
            assert!(url.contains("/00/")); // January = 0
            assert!(url.contains("/15/"));
            assert!(url.contains("14h_ticks.bi5"));
        }
    }

    mod timestamp_to_ms_from_hour {
        use super::*;

        #[test]
        fn test_start_of_hour() {
            let ts = Utc.with_ymd_and_hms(2024, 1, 1, 14, 0, 0).unwrap();
            assert_eq!(DukascopyFxService::timestamp_to_ms_from_hour(ts), 0);
        }

        #[test]
        fn test_mid_hour() {
            let ts = Utc.with_ymd_and_hms(2024, 1, 1, 14, 30, 0).unwrap();
            assert_eq!(
                DukascopyFxService::timestamp_to_ms_from_hour(ts),
                30 * 60 * 1000
            );
        }

        #[test]
        fn test_end_of_hour() {
            let ts = Utc.with_ymd_and_hms(2024, 1, 1, 14, 59, 59).unwrap();
            assert_eq!(
                DukascopyFxService::timestamp_to_ms_from_hour(ts),
                59 * 60 * 1000 + 59 * 1000
            );
        }
    }

    mod find_closest_tick {
        use super::*;
        use crate::instrument::InstrumentConfig;

        fn create_tick_data(ms: u32, ask: u32, bid: u32) -> Vec<u8> {
            let mut data = Vec::new();
            data.extend_from_slice(&ms.to_be_bytes());
            data.extend_from_slice(&ask.to_be_bytes());
            data.extend_from_slice(&bid.to_be_bytes());
            data.extend_from_slice(&1.0f32.to_be_bytes()); // ask_volume
            data.extend_from_slice(&1.0f32.to_be_bytes()); // bid_volume
            data
        }

        #[test]
        fn test_exact_match() {
            let mut data = Vec::new();
            data.extend(create_tick_data(1000, 110500, 110490));
            data.extend(create_tick_data(2000, 110510, 110500));
            data.extend(create_tick_data(3000, 110520, 110510));

            let result =
                DukascopyFxService::find_closest_tick(&data, 2000, InstrumentConfig::STANDARD);
            let tick = result.unwrap();

            assert_eq!(tick.ms_from_hour, 2000);
        }

        #[test]
        fn test_closest_match() {
            let mut data = Vec::new();
            data.extend(create_tick_data(1000, 110500, 110490));
            data.extend(create_tick_data(3000, 110520, 110510));

            let result =
                DukascopyFxService::find_closest_tick(&data, 2000, InstrumentConfig::STANDARD);
            let tick = result.unwrap();

            // Should match 1000 or 3000 (both are 1000ms away)
            assert!(tick.ms_from_hour == 1000 || tick.ms_from_hour == 3000);
        }

        #[test]
        fn test_empty_data() {
            let data: Vec<u8> = vec![];
            let result =
                DukascopyFxService::find_closest_tick(&data, 1000, InstrumentConfig::STANDARD);
            assert!(result.is_err());
        }
    }

    mod weekend_handling {
        use super::*;
        use crate::market::MARKET_CLOSE_HOUR;

        #[test]
        fn test_saturday_adjusts_to_friday() {
            let saturday = Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap();
            let adjusted = last_available_tick_time(saturday);

            assert_eq!(adjusted.weekday(), chrono::Weekday::Fri);
            assert_eq!(adjusted.hour(), MARKET_CLOSE_HOUR - 1);
        }

        #[test]
        fn test_sunday_morning_adjusts_to_friday() {
            let sunday = Utc.with_ymd_and_hms(2024, 1, 7, 10, 0, 0).unwrap();
            let adjusted = last_available_tick_time(sunday);

            assert_eq!(adjusted.weekday(), chrono::Weekday::Fri);
        }

        #[test]
        fn test_weekday_unchanged() {
            let wednesday = Utc.with_ymd_and_hms(2024, 1, 3, 14, 30, 0).unwrap();
            let adjusted = last_available_tick_time(wednesday);

            assert_eq!(adjusted, wednesday);
        }
    }
}
