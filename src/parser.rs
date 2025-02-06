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
