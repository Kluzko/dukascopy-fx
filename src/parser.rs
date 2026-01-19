//! Binary tick data parser for Dukascopy bi5 format.
//!
//! This module handles parsing of Dukascopy's proprietary binary tick data format.
//! Each tick is 20 bytes in big-endian format.

use crate::error::DukascopyError;
use crate::instrument::InstrumentConfig;
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;

/// Size of a single tick record in bytes
pub const TICK_SIZE_BYTES: usize = 20;

/// Parsed tick data from Dukascopy binary format
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedTick {
    /// Milliseconds from the start of the hour
    pub ms_from_hour: u32,
    /// Ask price (already converted using instrument config)
    pub ask: f64,
    /// Bid price (already converted using instrument config)
    pub bid: f64,
    /// Ask volume
    pub ask_volume: f32,
    /// Bid volume
    pub bid_volume: f32,
}

impl ParsedTick {
    /// Calculate the mid price (average of ask and bid)
    #[inline]
    pub fn mid_price(&self) -> f64 {
        (self.ask + self.bid) / 2.0
    }

    /// Calculate the spread (ask - bid)
    #[inline]
    pub fn spread(&self) -> f64 {
        self.ask - self.bid
    }
}

/// Parser for Dukascopy binary tick data
pub struct DukascopyParser;

impl DukascopyParser {
    /// Parse a single tick from binary data using the provided instrument configuration.
    ///
    /// # Binary Format (20 bytes, big-endian)
    /// - Bytes 0-3: Milliseconds from hour start (u32)
    /// - Bytes 4-7: Ask price as raw integer (u32)
    /// - Bytes 8-11: Bid price as raw integer (u32)
    /// - Bytes 12-15: Ask volume (f32)
    /// - Bytes 16-19: Bid volume (f32)
    ///
    /// # Arguments
    /// * `data` - Slice of exactly 20 bytes containing tick data
    /// * `config` - Instrument configuration with price divisor
    ///
    /// # Returns
    /// Parsed tick data or error if data is invalid
    ///
    /// # Errors
    /// - `DukascopyError::InvalidTickData` if data is malformed or contains invalid values
    pub fn parse_tick_with_config(
        data: &[u8],
        config: InstrumentConfig,
    ) -> Result<ParsedTick, DukascopyError> {
        if data.len() < TICK_SIZE_BYTES {
            return Err(DukascopyError::InvalidTickData);
        }

        let mut rdr = Cursor::new(data);
        let ms = rdr
            .read_u32::<BigEndian>()
            .map_err(|_| DukascopyError::InvalidTickData)?;
        let ask_raw = rdr
            .read_u32::<BigEndian>()
            .map_err(|_| DukascopyError::InvalidTickData)?;
        let bid_raw = rdr
            .read_u32::<BigEndian>()
            .map_err(|_| DukascopyError::InvalidTickData)?;
        let ask_volume = rdr
            .read_f32::<BigEndian>()
            .map_err(|_| DukascopyError::InvalidTickData)?;
        let bid_volume = rdr
            .read_f32::<BigEndian>()
            .map_err(|_| DukascopyError::InvalidTickData)?;

        let ask = ask_raw as f64 / config.price_divisor;
        let bid = bid_raw as f64 / config.price_divisor;

        // Validate prices and volumes
        if ask <= 0.0 || bid <= 0.0 {
            return Err(DukascopyError::InvalidTickData);
        }
        if ask_volume < 0.0 || bid_volume < 0.0 {
            return Err(DukascopyError::InvalidTickData);
        }
        // Ask should typically be >= bid (spread should be non-negative)
        // But we don't enforce this strictly as there might be edge cases

        Ok(ParsedTick {
            ms_from_hour: ms,
            ask,
            bid,
            ask_volume,
            bid_volume,
        })
    }

    /// Parse a single tick using default (standard forex) configuration.
    ///
    /// This is a convenience method that uses `InstrumentConfig::STANDARD` (divisor 100,000).
    /// For JPY pairs, metals, or other instruments, use `parse_tick_with_config` instead.
    ///
    /// # Deprecated
    /// This method exists for backward compatibility. Prefer `parse_tick_with_config`.
    #[deprecated(
        since = "0.2.0",
        note = "Use parse_tick_with_config with proper InstrumentConfig for accurate prices"
    )]
    pub fn parse_tick(data: &[u8]) -> Result<(u32, f64, f64, f32, f32), DukascopyError> {
        let tick = Self::parse_tick_with_config(data, InstrumentConfig::STANDARD)?;
        Ok((
            tick.ms_from_hour,
            tick.ask,
            tick.bid,
            tick.ask_volume,
            tick.bid_volume,
        ))
    }

    /// Validate that decompressed data has correct size (multiple of tick size).
    ///
    /// # Arguments
    /// * `data` - Decompressed tick data bytes
    ///
    /// # Returns
    /// `Ok(())` if valid, `Err(InvalidTickData)` if not a multiple of 20 bytes
    pub fn validate_decompressed_data(data: &[u8]) -> Result<(), DukascopyError> {
        if data.is_empty() {
            return Err(DukascopyError::DataNotFound);
        }
        if !data.len().is_multiple_of(TICK_SIZE_BYTES) {
            return Err(DukascopyError::InvalidTickData);
        }
        Ok(())
    }

    /// Returns the number of ticks in the decompressed data.
    ///
    /// # Arguments
    /// * `data` - Decompressed tick data bytes
    ///
    /// # Returns
    /// Number of complete ticks in the data
    #[inline]
    pub fn tick_count(data: &[u8]) -> usize {
        data.len() / TICK_SIZE_BYTES
    }

    /// Iterator over ticks in decompressed data.
    ///
    /// # Arguments
    /// * `data` - Decompressed tick data bytes
    /// * `config` - Instrument configuration for price conversion
    ///
    /// # Returns
    /// Iterator yielding `Result<ParsedTick, DukascopyError>` for each tick
    pub fn iter_ticks(
        data: &[u8],
        config: InstrumentConfig,
    ) -> impl Iterator<Item = Result<ParsedTick, DukascopyError>> + '_ {
        data.chunks_exact(TICK_SIZE_BYTES)
            .map(move |chunk| Self::parse_tick_with_config(chunk, config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::InstrumentConfig;

    // Helper to create valid tick data
    fn create_tick_data(ms: u32, ask: u32, bid: u32, ask_vol: f32, bid_vol: f32) -> Vec<u8> {
        let mut data = Vec::with_capacity(TICK_SIZE_BYTES);
        data.extend_from_slice(&ms.to_be_bytes());
        data.extend_from_slice(&ask.to_be_bytes());
        data.extend_from_slice(&bid.to_be_bytes());
        data.extend_from_slice(&ask_vol.to_be_bytes());
        data.extend_from_slice(&bid_vol.to_be_bytes());
        data
    }

    mod parse_tick_with_config {
        use super::*;

        #[test]
        fn test_parse_standard_forex_tick() {
            // EUR/USD at 1.10500 (ask) and 1.10490 (bid)
            let data = create_tick_data(1000, 110500, 110490, 1.5, 2.0);
            let tick =
                DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::STANDARD).unwrap();

            assert_eq!(tick.ms_from_hour, 1000);
            assert!((tick.ask - 1.10500).abs() < 0.00001);
            assert!((tick.bid - 1.10490).abs() < 0.00001);
            assert!((tick.ask_volume - 1.5).abs() < 0.0001);
            assert!((tick.bid_volume - 2.0).abs() < 0.0001);
        }

        #[test]
        fn test_parse_jpy_tick() {
            // USD/JPY at 150.250 (ask) and 150.240 (bid)
            let data = create_tick_data(2000, 150250, 150240, 3.0, 4.0);
            let tick =
                DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::JPY).unwrap();

            assert_eq!(tick.ms_from_hour, 2000);
            assert!((tick.ask - 150.250).abs() < 0.001);
            assert!((tick.bid - 150.240).abs() < 0.001);
        }

        #[test]
        fn test_parse_gold_tick() {
            // XAU/USD at 2050.500 (ask) and 2050.250 (bid)
            let data = create_tick_data(3000, 2050500, 2050250, 0.5, 0.5);
            let tick =
                DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::METALS).unwrap();

            assert_eq!(tick.ms_from_hour, 3000);
            assert!((tick.ask - 2050.500).abs() < 0.001);
            assert!((tick.bid - 2050.250).abs() < 0.001);
        }

        #[test]
        fn test_parse_rub_tick() {
            // USD/RUB at 90.500 (ask) and 90.400 (bid)
            let data = create_tick_data(4000, 90500, 90400, 1.0, 1.0);
            let tick =
                DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::RUB).unwrap();

            assert!((tick.ask - 90.500).abs() < 0.001);
            assert!((tick.bid - 90.400).abs() < 0.001);
        }

        #[test]
        fn test_zero_volume_is_valid() {
            let data = create_tick_data(1000, 110500, 110490, 0.0, 0.0);
            let result = DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::STANDARD);
            assert!(result.is_ok());
        }

        #[test]
        fn test_zero_price_is_invalid() {
            let data = create_tick_data(1000, 0, 110490, 1.0, 1.0);
            let result = DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::STANDARD);
            assert!(matches!(result, Err(DukascopyError::InvalidTickData)));
        }

        #[test]
        fn test_negative_volume_is_invalid() {
            let data = create_tick_data(1000, 110500, 110490, -1.0, 1.0);
            let result = DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::STANDARD);
            assert!(matches!(result, Err(DukascopyError::InvalidTickData)));
        }

        #[test]
        fn test_incomplete_data() {
            let data = vec![0u8; 19]; // One byte short
            let result = DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::STANDARD);
            assert!(matches!(result, Err(DukascopyError::InvalidTickData)));
        }

        #[test]
        fn test_empty_data() {
            let data: Vec<u8> = vec![];
            let result = DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::STANDARD);
            assert!(matches!(result, Err(DukascopyError::InvalidTickData)));
        }
    }

    mod parsed_tick {
        use super::*;

        #[test]
        fn test_mid_price() {
            let tick = ParsedTick {
                ms_from_hour: 0,
                ask: 1.10500,
                bid: 1.10400,
                ask_volume: 1.0,
                bid_volume: 1.0,
            };
            assert!((tick.mid_price() - 1.10450).abs() < 0.00001);
        }

        #[test]
        fn test_spread() {
            let tick = ParsedTick {
                ms_from_hour: 0,
                ask: 1.10500,
                bid: 1.10400,
                ask_volume: 1.0,
                bid_volume: 1.0,
            };
            assert!((tick.spread() - 0.00100).abs() < 0.00001);
        }
    }

    mod validate_decompressed_data {
        use super::*;

        #[test]
        fn test_valid_single_tick() {
            let data = vec![0u8; 20];
            assert!(DukascopyParser::validate_decompressed_data(&data).is_ok());
        }

        #[test]
        fn test_valid_multiple_ticks() {
            let data = vec![0u8; 60]; // 3 ticks
            assert!(DukascopyParser::validate_decompressed_data(&data).is_ok());
        }

        #[test]
        fn test_invalid_size() {
            let data = vec![0u8; 19];
            assert!(matches!(
                DukascopyParser::validate_decompressed_data(&data),
                Err(DukascopyError::InvalidTickData)
            ));
        }

        #[test]
        fn test_invalid_partial_tick() {
            let data = vec![0u8; 25]; // 1 tick + 5 extra bytes
            assert!(matches!(
                DukascopyParser::validate_decompressed_data(&data),
                Err(DukascopyError::InvalidTickData)
            ));
        }

        #[test]
        fn test_empty_data() {
            let data: Vec<u8> = vec![];
            assert!(matches!(
                DukascopyParser::validate_decompressed_data(&data),
                Err(DukascopyError::DataNotFound)
            ));
        }
    }

    mod tick_count {
        use super::*;

        #[test]
        fn test_tick_count() {
            assert_eq!(DukascopyParser::tick_count(&[0u8; 0]), 0);
            assert_eq!(DukascopyParser::tick_count(&[0u8; 20]), 1);
            assert_eq!(DukascopyParser::tick_count(&[0u8; 40]), 2);
            assert_eq!(DukascopyParser::tick_count(&[0u8; 100]), 5);
        }

        #[test]
        fn test_tick_count_partial() {
            // Partial ticks are ignored
            assert_eq!(DukascopyParser::tick_count(&[0u8; 25]), 1);
            assert_eq!(DukascopyParser::tick_count(&[0u8; 19]), 0);
        }
    }

    mod iter_ticks {
        use super::*;

        #[test]
        fn test_iter_empty() {
            let data: Vec<u8> = vec![];
            let count = DukascopyParser::iter_ticks(&data, InstrumentConfig::STANDARD).count();
            assert_eq!(count, 0);
        }

        #[test]
        fn test_iter_multiple_ticks() {
            let mut data = Vec::new();
            data.extend(create_tick_data(1000, 110500, 110490, 1.0, 1.0));
            data.extend(create_tick_data(2000, 110510, 110500, 1.0, 1.0));
            data.extend(create_tick_data(3000, 110520, 110510, 1.0, 1.0));

            let ticks: Vec<_> = DukascopyParser::iter_ticks(&data, InstrumentConfig::STANDARD)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            assert_eq!(ticks.len(), 3);
            assert_eq!(ticks[0].ms_from_hour, 1000);
            assert_eq!(ticks[1].ms_from_hour, 2000);
            assert_eq!(ticks[2].ms_from_hour, 3000);
        }
    }

    mod backward_compatibility {
        use super::*;

        #[test]
        #[allow(deprecated)]
        fn test_deprecated_parse_tick() {
            let data = create_tick_data(1000, 110500, 110490, 1.5, 2.0);
            let (ms, ask, bid, ask_vol, bid_vol) = DukascopyParser::parse_tick(&data).unwrap();

            assert_eq!(ms, 1000);
            assert!((ask - 1.10500).abs() < 0.00001);
            assert!((bid - 1.10490).abs() < 0.00001);
            assert!((ask_vol - 1.5).abs() < 0.0001);
            assert!((bid_vol - 2.0).abs() < 0.0001);
        }
    }
}
