use std::io;
use thiserror::Error;
use tokio::task::JoinError;

#[derive(Error, Debug)]
pub enum DukascopyError {
    #[error("HTTP error: {0}")]
    HttpError(String),

    #[error("LZMA decompression error: {0}")]
    LzmaError(String),

    #[error("Invalid tick data")]
    InvalidTickData,

    #[error("Invalid currency code")]
    InvalidCurrencyCode,

    #[error("Market is closed on weekends")]
    MarketClosed,

    #[error("Data not found for the specified hour")]
    DataNotFound,

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Unauthorized access")]
    Unauthorized,

    #[error("Access forbidden")]
    Forbidden,

    #[error("Invalid request")]
    InvalidRequest,

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<reqwest::Error> for DukascopyError {
    fn from(err: reqwest::Error) -> Self {
        DukascopyError::HttpError(err.to_string())
    }
}

impl From<lzma_rs::error::Error> for DukascopyError {
    fn from(err: lzma_rs::error::Error) -> Self {
        DukascopyError::LzmaError(err.to_string())
    }
}

impl From<io::Error> for DukascopyError {
    fn from(err: io::Error) -> Self {
        DukascopyError::Unknown(format!("IO error: {}", err))
    }
}

impl From<JoinError> for DukascopyError {
    fn from(err: JoinError) -> Self {
        DukascopyError::Unknown(format!("Task join error: {}", err))
    }
}
