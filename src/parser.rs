use crate::error::DukascopyError;
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;

pub struct DukascopyParser;

impl DukascopyParser {
    pub fn parse_tick(data: &[u8]) -> Result<(u32, f64, f64, f32, f32), DukascopyError> {
        let mut rdr = Cursor::new(data);
        let ms = rdr.read_u32::<BigEndian>()?;
        let ask = rdr.read_u32::<BigEndian>()? as f64 / 100_000.0;
        let bid = rdr.read_u32::<BigEndian>()? as f64 / 100_000.0;
        let ask_volume = rdr.read_f32::<BigEndian>()?;
        let bid_volume = rdr.read_f32::<BigEndian>()?;

        if ask <= 0.0 || bid <= 0.0 || ask_volume < 0.0 || bid_volume < 0.0 {
            return Err(DukascopyError::InvalidTickData);
        }

        Ok((ms, ask, bid, ask_volume, bid_volume))
    }

    pub fn validate_decompressed_data(data: &[u8]) -> Result<(), DukascopyError> {
        if data.len() % 20 != 0 {
            return Err(DukascopyError::InvalidTickData);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tick() {
        let data = vec![
            0x00, 0x00, 0x00, 0x01, // ms = 1
            0x00, 0x00, 0x00, 0x02, // ask = 2
            0x00, 0x00, 0x00, 0x03, // bid = 3
            0x40, 0x00, 0x00, 0x00, // ask_volume = 2.0
            0x40, 0x80, 0x00, 0x00, // bid_volume = 4.0
        ];

        let result = DukascopyParser::parse_tick(&data).unwrap();
        assert_eq!(result, (1, 0.00002, 0.00003, 2.0, 4.0));
    }

    #[test]
    fn test_parse_tick_invalid_data() {
        let data = vec![0x00; 19]; // Incomplete data
        let result = DukascopyParser::parse_tick(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_decompressed_data() {
        let valid_data = vec![0x00; 20]; // Valid data (multiple of 20)
        let invalid_data = vec![0x00; 19]; // Invalid data (not multiple of 20)

        assert!(DukascopyParser::validate_decompressed_data(&valid_data).is_ok());
        assert!(DukascopyParser::validate_decompressed_data(&invalid_data).is_err());
    }
}
