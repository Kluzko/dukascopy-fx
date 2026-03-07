//! Data sink abstractions for fetcher output.

use crate::error::DukascopyError;
use crate::models::CurrencyExchange;
#[cfg(feature = "sinks-parquet")]
use arrow::array::{ArrayRef, Float32Array, Float64Array, StringArray, TimestampMillisecondArray};
#[cfg(feature = "sinks-parquet")]
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
#[cfg(feature = "sinks-parquet")]
use arrow::record_batch::RecordBatch;
#[cfg(feature = "sinks-parquet")]
use parquet::arrow::ArrowWriter;
#[cfg(feature = "sinks-parquet")]
use parquet::file::properties::WriterProperties;
#[cfg(feature = "sinks-parquet")]
use rust_decimal::prelude::ToPrimitive;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
#[cfg(feature = "sinks-parquet")]
use std::sync::Arc;

#[cfg(feature = "sinks-parquet")]
const DEFAULT_PARQUET_FLUSH_ROWS: usize = 50_000;
#[cfg(feature = "sinks-parquet")]
const PART_FILE_PREFIX: &str = "part-";
#[cfg(feature = "sinks-parquet")]
const PART_FILE_SUFFIX: &str = ".parquet";

/// Output sink interface used by fetcher jobs.
pub trait DataSink {
    /// Writes batch of rows for a given symbol.
    fn write_batch(
        &mut self,
        symbol: &str,
        rows: &[CurrencyExchange],
    ) -> Result<usize, DukascopyError>;

    /// Flushes buffered content.
    fn flush(&mut self) -> Result<(), DukascopyError>;
}

/// No-op sink implementation.
#[derive(Debug, Default)]
pub struct NoopSink;

impl DataSink for NoopSink {
    fn write_batch(
        &mut self,
        _symbol: &str,
        rows: &[CurrencyExchange],
    ) -> Result<usize, DukascopyError> {
        Ok(rows.len())
    }

    fn flush(&mut self) -> Result<(), DukascopyError> {
        Ok(())
    }
}

/// CSV sink for fetched rows.
pub struct CsvSink {
    path: PathBuf,
    writer: csv::Writer<File>,
}

#[derive(Debug, Clone)]
#[cfg(feature = "sinks-parquet")]
struct SinkRow {
    symbol: String,
    base: String,
    quote: String,
    timestamp_ms: i64,
    rate: f64,
    bid: f64,
    ask: f64,
    bid_volume: f32,
    ask_volume: f32,
}

#[cfg(feature = "sinks-parquet")]
fn decimal_to_f64(value: &rust_decimal::Decimal) -> Result<f64, DukascopyError> {
    value.to_f64().ok_or_else(|| {
        DukascopyError::InvalidRequest(format!(
            "Decimal value '{}' cannot be represented as f64",
            value
        ))
    })
}

#[cfg(feature = "sinks-parquet")]
fn to_sink_row(symbol: &str, row: &CurrencyExchange) -> Result<SinkRow, DukascopyError> {
    Ok(SinkRow {
        symbol: symbol.to_string(),
        base: row.pair.from().to_string(),
        quote: row.pair.to().to_string(),
        timestamp_ms: row.timestamp.timestamp_millis(),
        rate: decimal_to_f64(&row.rate)?,
        bid: decimal_to_f64(&row.bid)?,
        ask: decimal_to_f64(&row.ask)?,
        bid_volume: row.bid_volume,
        ask_volume: row.ask_volume,
    })
}

/// Parquet sink for fetched rows.
///
/// The sink stores data as a parquet dataset directory (`part-*.parquet`).
/// This makes incremental runs append-only and avoids data loss caused by
/// truncating a single parquet file on each run.
#[cfg(feature = "sinks-parquet")]
pub struct ParquetSink {
    path: PathBuf,
    rows: Vec<SinkRow>,
    flush_rows_threshold: usize,
    next_part_index: u64,
}

#[cfg(feature = "sinks-parquet")]
impl ParquetSink {
    /// Creates a parquet sink. Data is written in parts on `flush()`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DukascopyError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                DukascopyError::Unknown(format!(
                    "Failed to create sink directory '{}': {}",
                    parent.display(),
                    err
                ))
            })?;
        }

        Self::ensure_dataset_dir(&path)?;
        let next_part_index = Self::detect_next_part_index(&path)?;

        Ok(Self {
            path,
            rows: Vec::new(),
            flush_rows_threshold: DEFAULT_PARQUET_FLUSH_ROWS,
            next_part_index,
        })
    }

    /// Returns sink path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Sets in-memory row threshold that triggers automatic flush.
    pub fn flush_rows_threshold(mut self, threshold: usize) -> Self {
        self.flush_rows_threshold = threshold.max(1);
        self
    }

    fn ensure_dataset_dir(path: &Path) -> Result<(), DukascopyError> {
        if path.exists() {
            if path.is_dir() {
                return Ok(());
            }
            Self::migrate_legacy_file_to_dataset(path)?;
            return Ok(());
        }

        fs::create_dir_all(path).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to create parquet dataset directory '{}': {}",
                path.display(),
                err
            ))
        })
    }

    fn migrate_legacy_file_to_dataset(path: &Path) -> Result<(), DukascopyError> {
        let parent = path.parent().ok_or_else(|| {
            DukascopyError::Unknown(format!(
                "Invalid parquet output path '{}': missing parent directory",
                path.display()
            ))
        })?;

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                DukascopyError::Unknown(format!(
                    "Invalid parquet output path '{}': non-utf8 file name",
                    path.display()
                ))
            })?;

        let legacy_file_path = parent.join(format!("{}.legacy", file_name));
        fs::rename(path, &legacy_file_path).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to migrate legacy parquet file '{}': {}",
                path.display(),
                err
            ))
        })?;

        fs::create_dir_all(path).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to create parquet dataset directory '{}': {}",
                path.display(),
                err
            ))
        })?;

        let migrated_part = path.join(format!("{}{:06}{}", PART_FILE_PREFIX, 0, PART_FILE_SUFFIX));
        fs::rename(&legacy_file_path, &migrated_part).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to move legacy parquet file '{}' into dataset '{}': {}",
                legacy_file_path.display(),
                migrated_part.display(),
                err
            ))
        })
    }

    fn detect_next_part_index(path: &Path) -> Result<u64, DukascopyError> {
        let mut max_index: Option<u64> = None;
        let entries = fs::read_dir(path).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to read parquet dataset directory '{}': {}",
                path.display(),
                err
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|err| {
                DukascopyError::Unknown(format!(
                    "Failed to inspect parquet dataset entry in '{}': {}",
                    path.display(),
                    err
                ))
            })?;
            let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let Some(index) = Self::parse_part_index(&name) else {
                continue;
            };
            max_index = Some(max_index.map_or(index, |current| current.max(index)));
        }

        Ok(max_index.map_or(0, |index| index + 1))
    }

    fn parse_part_index(file_name: &str) -> Option<u64> {
        if !file_name.starts_with(PART_FILE_PREFIX) || !file_name.ends_with(PART_FILE_SUFFIX) {
            return None;
        }

        let numeric = &file_name[PART_FILE_PREFIX.len()..file_name.len() - PART_FILE_SUFFIX.len()];
        numeric.parse::<u64>().ok()
    }

    fn next_part_path(&mut self) -> PathBuf {
        loop {
            let candidate = self.path.join(format!(
                "{}{:06}{}",
                PART_FILE_PREFIX, self.next_part_index, PART_FILE_SUFFIX
            ));
            self.next_part_index = self.next_part_index.saturating_add(1);
            if !candidate.exists() {
                return candidate;
            }
        }
    }

    fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("symbol", DataType::Utf8, false),
            Field::new("base", DataType::Utf8, false),
            Field::new("quote", DataType::Utf8, false),
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("rate", DataType::Float64, false),
            Field::new("bid", DataType::Float64, false),
            Field::new("ask", DataType::Float64, false),
            Field::new("bid_volume", DataType::Float32, false),
            Field::new("ask_volume", DataType::Float32, false),
        ]))
    }

    fn build_batch(rows: &[SinkRow]) -> Result<RecordBatch, DukascopyError> {
        let schema = Self::schema();

        let symbol: Vec<&str> = rows.iter().map(|row| row.symbol.as_str()).collect();
        let base: Vec<&str> = rows.iter().map(|row| row.base.as_str()).collect();
        let quote: Vec<&str> = rows.iter().map(|row| row.quote.as_str()).collect();
        let timestamp: Vec<i64> = rows.iter().map(|row| row.timestamp_ms).collect();
        let rate: Vec<f64> = rows.iter().map(|row| row.rate).collect();
        let bid: Vec<f64> = rows.iter().map(|row| row.bid).collect();
        let ask: Vec<f64> = rows.iter().map(|row| row.ask).collect();
        let bid_volume: Vec<f32> = rows.iter().map(|row| row.bid_volume).collect();
        let ask_volume: Vec<f32> = rows.iter().map(|row| row.ask_volume).collect();

        let arrays: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(symbol)),
            Arc::new(StringArray::from(base)),
            Arc::new(StringArray::from(quote)),
            Arc::new(TimestampMillisecondArray::from(timestamp)),
            Arc::new(Float64Array::from(rate)),
            Arc::new(Float64Array::from(bid)),
            Arc::new(Float64Array::from(ask)),
            Arc::new(Float32Array::from(bid_volume)),
            Arc::new(Float32Array::from(ask_volume)),
        ];

        RecordBatch::try_new(schema, arrays).map_err(|err| {
            DukascopyError::Unknown(format!("Failed to build parquet batch: {}", err))
        })
    }

    fn write_rows_to_parquet(path: &Path, rows: &[SinkRow]) -> Result<(), DukascopyError> {
        let file = File::create(path).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to create parquet part file '{}': {}",
                path.display(),
                err
            ))
        })?;

        let props = WriterProperties::builder().build();
        let schema = Self::schema();
        let mut writer = ArrowWriter::try_new(file, schema, Some(props)).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to initialize parquet writer '{}': {}",
                path.display(),
                err
            ))
        })?;

        let batch = Self::build_batch(rows)?;
        writer.write(&batch).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to write parquet batch '{}': {}",
                path.display(),
                err
            ))
        })?;

        writer.close().map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to close parquet writer '{}': {}",
                path.display(),
                err
            ))
        })?;

        Ok(())
    }

    fn deduplicate_rows(rows: &mut Vec<SinkRow>) {
        rows.sort_by(|a, b| {
            a.symbol
                .cmp(&b.symbol)
                .then(a.timestamp_ms.cmp(&b.timestamp_ms))
        });
        rows.dedup_by(|a, b| a.symbol == b.symbol && a.timestamp_ms == b.timestamp_ms);
    }
}

impl CsvSink {
    /// Creates or appends to a CSV sink.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DukascopyError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                DukascopyError::Unknown(format!(
                    "Failed to create sink directory '{}': {}",
                    parent.display(),
                    err
                ))
            })?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|err| {
                DukascopyError::Unknown(format!(
                    "Failed to open CSV sink file '{}': {}",
                    path.display(),
                    err
                ))
            })?;

        let writer = csv::WriterBuilder::new().from_writer(file);

        Ok(Self { path, writer })
    }

    /// Returns sink path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl DataSink for CsvSink {
    fn write_batch(
        &mut self,
        symbol: &str,
        rows: &[CurrencyExchange],
    ) -> Result<usize, DukascopyError> {
        for row in rows {
            self.writer
                .write_record([
                    symbol,
                    row.pair.from(),
                    row.pair.to(),
                    &row.timestamp.to_rfc3339(),
                    &row.rate.to_string(),
                    &row.bid.to_string(),
                    &row.ask.to_string(),
                    &row.bid_volume.to_string(),
                    &row.ask_volume.to_string(),
                ])
                .map_err(|err| {
                    DukascopyError::Unknown(format!(
                        "Failed to write row to CSV sink '{}': {}",
                        self.path.display(),
                        err
                    ))
                })?;
        }
        Ok(rows.len())
    }

    fn flush(&mut self) -> Result<(), DukascopyError> {
        self.writer.flush().map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to flush CSV sink '{}': {}",
                self.path.display(),
                err
            ))
        })
    }
}

#[cfg(feature = "sinks-parquet")]
impl DataSink for ParquetSink {
    fn write_batch(
        &mut self,
        symbol: &str,
        rows: &[CurrencyExchange],
    ) -> Result<usize, DukascopyError> {
        self.rows.reserve(rows.len());
        for row in rows {
            self.rows.push(to_sink_row(symbol, row)?);
        }

        if self.rows.len() >= self.flush_rows_threshold {
            self.flush()?;
        }

        Ok(rows.len())
    }

    fn flush(&mut self) -> Result<(), DukascopyError> {
        if self.rows.is_empty() {
            return Ok(());
        }

        Self::deduplicate_rows(&mut self.rows);
        let part_path = self.next_part_path();
        Self::write_rows_to_parquet(&part_path, &self.rows)?;
        self.rows.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CurrencyPair;
    use chrono::TimeZone;
    #[cfg(feature = "sinks-parquet")]
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn sample_exchange(base: &str, quote: &str, ts_hour: u32, rate: &str) -> CurrencyExchange {
        CurrencyExchange {
            pair: CurrencyPair::new(base, quote),
            rate: Decimal::from_str(rate).unwrap(),
            timestamp: chrono::Utc
                .with_ymd_and_hms(2025, 1, 3, ts_hour, 0, 0)
                .unwrap(),
            ask: Decimal::from_str(rate).unwrap() + Decimal::from_str("0.00010").unwrap(),
            bid: Decimal::from_str(rate).unwrap() - Decimal::from_str("0.00010").unwrap(),
            ask_volume: 1.0,
            bid_volume: 1.0,
        }
    }

    #[cfg(feature = "sinks-parquet")]
    fn dataset_row_count(path: &Path) -> i64 {
        let mut total = 0_i64;
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if !entry.path().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(PART_FILE_SUFFIX) {
                continue;
            }
            let file = File::open(entry.path()).unwrap();
            let reader = SerializedFileReader::new(file).unwrap();
            total += reader.metadata().file_metadata().num_rows();
        }
        total
    }

    #[test]
    fn test_csv_sink_writes_rows() {
        let unique = format!(
            "dukascopy_fx_sink_test_{}.csv",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);

        let mut sink = CsvSink::open(&path).unwrap();
        let rows = vec![sample_exchange("EUR", "USD", 12, "1.10000")];

        let written = sink.write_batch("EURUSD", &rows).unwrap();
        assert_eq!(written, 1);
        sink.flush().unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("EURUSD"));
        assert!(content.contains("1.10000"));

        let _ = fs::remove_file(path);
    }

    #[test]
    #[cfg(feature = "sinks-parquet")]
    fn test_parquet_sink_writes_rows() {
        let unique = format!(
            "dukascopy_fx_sink_dataset_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);

        let mut sink = ParquetSink::open(&path).unwrap();
        let rows = vec![sample_exchange("EUR", "USD", 12, "1.10000")];

        let written = sink.write_batch("EURUSD", &rows).unwrap();
        assert_eq!(written, 1);
        sink.flush().unwrap();

        assert!(path.is_dir());
        assert_eq!(dataset_row_count(&path), 1);

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    #[cfg(feature = "sinks-parquet")]
    fn test_parquet_sink_appends_on_multiple_flushes() {
        let unique = format!(
            "dukascopy_fx_sink_append_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);

        let mut sink = ParquetSink::open(&path).unwrap().flush_rows_threshold(1);

        sink.write_batch("EURUSD", &[sample_exchange("EUR", "USD", 12, "1.10000")])
            .unwrap();
        sink.write_batch("EURUSD", &[sample_exchange("EUR", "USD", 13, "1.10100")])
            .unwrap();
        sink.flush().unwrap();

        let part_files = fs::read_dir(&path)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(PART_FILE_SUFFIX)
            })
            .count();

        assert!(part_files >= 2);
        assert_eq!(dataset_row_count(&path), 2);

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    #[cfg(feature = "sinks-parquet")]
    fn test_parquet_sink_migrates_legacy_file_without_data_loss() {
        let unique = format!(
            "dukascopy_fx_sink_legacy_test_{}.parquet",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);

        let legacy_rows =
            vec![to_sink_row("EURUSD", &sample_exchange("EUR", "USD", 12, "1.10000")).unwrap()];
        ParquetSink::write_rows_to_parquet(&path, &legacy_rows).unwrap();

        let mut sink = ParquetSink::open(&path).unwrap();
        sink.write_batch("EURUSD", &[sample_exchange("EUR", "USD", 13, "1.10100")])
            .unwrap();
        sink.flush().unwrap();

        assert!(path.is_dir());
        assert_eq!(dataset_row_count(&path), 2);

        let _ = fs::remove_dir_all(path);
    }
}
