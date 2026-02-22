//! Binary tick data parser for Dukascopy bi5 format.

use crate::core::instrument::InstrumentConfig;
use crate::error::DukascopyError;
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;

/// Size of a single tick record in bytes
pub const TICK_SIZE_BYTES: usize = 20;

/// Number of milliseconds in one hour
pub const MILLIS_PER_HOUR: u32 = 3_600_000;

/// Parsed tick data from Dukascopy binary format
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedTick {
    /// Milliseconds from the start of the hour
    pub ms_from_hour: u32,
    /// Ask price (converted using instrument config)
    pub ask: f64,
    /// Bid price (converted using instrument config)
    pub bid: f64,
    /// Ask volume
    pub ask_volume: f32,
    /// Bid volume
    pub bid_volume: f32,
}

impl ParsedTick {
    #[inline]
    pub fn mid_price(&self) -> f64 {
        (self.ask + self.bid) / 2.0
    }

    #[inline]
    pub fn spread(&self) -> f64 {
        self.ask - self.bid
    }
}

/// Parser for Dukascopy binary tick data
pub struct DukascopyParser;

impl DukascopyParser {
    /// Parse a single tick from binary data.
    ///
    /// # Binary Format (20 bytes, big-endian)
    /// - Bytes 0-3: Milliseconds from hour start (u32)
    /// - Bytes 4-7: Ask price as raw integer (u32)
    /// - Bytes 8-11: Bid price as raw integer (u32)
    /// - Bytes 12-15: Ask volume (f32)
    /// - Bytes 16-19: Bid volume (f32)
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

        if ms >= MILLIS_PER_HOUR || ask <= 0.0 || bid <= 0.0 || ask_volume < 0.0 || bid_volume < 0.0
        {
            return Err(DukascopyError::InvalidTickData);
        }

        Ok(ParsedTick {
            ms_from_hour: ms,
            ask,
            bid,
            ask_volume,
            bid_volume,
        })
    }

    /// Validate that decompressed data has correct size.
    pub fn validate_decompressed_data(data: &[u8]) -> Result<(), DukascopyError> {
        if data.is_empty() {
            return Err(DukascopyError::DataNotFound);
        }
        if !data.len().is_multiple_of(TICK_SIZE_BYTES) {
            return Err(DukascopyError::InvalidTickData);
        }
        Ok(())
    }

    /// Returns the number of ticks in the data.
    #[inline]
    pub fn tick_count(data: &[u8]) -> usize {
        data.len() / TICK_SIZE_BYTES
    }

    /// Iterator over ticks in decompressed data.
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

    fn create_tick_data(ms: u32, ask: u32, bid: u32, ask_vol: f32, bid_vol: f32) -> Vec<u8> {
        let mut data = Vec::with_capacity(TICK_SIZE_BYTES);
        data.extend_from_slice(&ms.to_be_bytes());
        data.extend_from_slice(&ask.to_be_bytes());
        data.extend_from_slice(&bid.to_be_bytes());
        data.extend_from_slice(&ask_vol.to_be_bytes());
        data.extend_from_slice(&bid_vol.to_be_bytes());
        data
    }

    #[test]
    fn test_parse_standard_tick() {
        let data = create_tick_data(1000, 110500, 110490, 1.5, 2.0);
        let tick =
            DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::STANDARD).unwrap();

        assert_eq!(tick.ms_from_hour, 1000);
        assert!((tick.ask - 1.10500).abs() < 0.00001);
        assert!((tick.bid - 1.10490).abs() < 0.00001);
    }

    #[test]
    fn test_parse_jpy_tick() {
        let data = create_tick_data(2000, 150250, 150240, 3.0, 4.0);
        let tick = DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::JPY).unwrap();

        assert!((tick.ask - 150.250).abs() < 0.001);
        assert!((tick.bid - 150.240).abs() < 0.001);
    }

    #[test]
    fn test_parse_tick_invalid_ms_range() {
        let data = create_tick_data(MILLIS_PER_HOUR, 110500, 110490, 1.5, 2.0);
        assert!(
            DukascopyParser::parse_tick_with_config(&data, InstrumentConfig::STANDARD).is_err()
        );
    }

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
    fn test_validate_data() {
        assert!(DukascopyParser::validate_decompressed_data(&[0u8; 20]).is_ok());
        assert!(DukascopyParser::validate_decompressed_data(&[0u8; 19]).is_err());
        assert!(DukascopyParser::validate_decompressed_data(&[]).is_err());
    }

    #[test]
    fn test_tick_count() {
        assert_eq!(DukascopyParser::tick_count(&[0u8; 40]), 2);
        assert_eq!(DukascopyParser::tick_count(&[0u8; 25]), 1);
    }
}
