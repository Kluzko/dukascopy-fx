//! Error types for the Dukascopy FX library.

use std::io;
use thiserror::Error;
use tokio::task::JoinError;

/// Transport-layer error category for machine-actionable handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportErrorKind {
    /// Request timed out.
    Timeout,
    /// Connection establishment failed.
    Connect,
    /// Non-success HTTP status code not covered by dedicated variants.
    HttpStatus,
    /// Failed reading HTTP response body.
    ResponseBody,
    /// Other transport-level request/response failure.
    Other,
}

/// Errors that can occur when using the Dukascopy FX library.
#[derive(Error, Debug, Clone)]
pub enum DukascopyError {
    /// Structured transport/network error.
    #[error("Transport error ({kind:?}, status={status:?}): {message}")]
    Transport {
        /// Transport error category.
        kind: TransportErrorKind,
        /// Optional HTTP status code for status-based failures.
        status: Option<u16>,
        /// Human-readable transport error details.
        message: String,
    },

    /// LZMA decompression failed
    #[error("LZMA decompression error: {0}")]
    LzmaError(String),

    /// Tick data is malformed or invalid
    #[error("Invalid tick data: data is malformed or contains invalid values")]
    InvalidTickData,

    /// Invalid currency code provided
    #[error("Invalid currency code '{code}': {reason}")]
    InvalidCurrencyCode {
        /// The invalid currency code
        code: String,
        /// Reason why it's invalid
        reason: String,
    },

    /// No data available for the requested time/pair
    #[error("Data not found for {pair} at {timestamp}")]
    DataNotFoundFor {
        /// The currency pair requested
        pair: String,
        /// The timestamp requested
        timestamp: String,
    },

    /// Generic data not found (for backward compatibility)
    #[error("Data not found for the specified time")]
    DataNotFound,

    /// API rate limit exceeded
    #[error("Rate limit exceeded. Please wait before making more requests.")]
    RateLimitExceeded,

    /// Unauthorized access (HTTP 401)
    #[error("Unauthorized access")]
    Unauthorized,

    /// Forbidden access (HTTP 403)
    #[error("Access forbidden")]
    Forbidden,

    /// Invalid request (HTTP 400)
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Missing configured default quote currency for symbol-only request.
    #[error("Missing default quote currency in client configuration")]
    MissingDefaultQuoteCurrency,

    /// Symbol-only resolution is disabled in client configuration.
    #[error("Symbol-only pair resolution is disabled in client configuration")]
    PairResolutionDisabled,

    /// No available direct or synthetic route for symbol conversion.
    #[error("No conversion route found for {symbol}/{quote}")]
    NoConversionRoute { symbol: String, quote: String },

    /// Request timeout
    #[error("Request timed out after {0} seconds")]
    Timeout(u64),

    /// Cache error
    #[error("Cache error: {0}")]
    CacheError(String),

    /// Unknown error with context
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl DukascopyError {
    /// Returns true if this error is retryable.
    ///
    /// Retryable errors are transient and may succeed on retry:
    /// - Rate limiting
    /// - Timeouts
    /// - Some HTTP errors
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::RateLimitExceeded | Self::Timeout(_) => true,
            Self::Transport { kind, status, .. } => match kind {
                TransportErrorKind::Timeout | TransportErrorKind::Connect => true,
                TransportErrorKind::HttpStatus => status
                    .map(|code| code == 429 || (500..=599).contains(&code))
                    .unwrap_or(false),
                TransportErrorKind::ResponseBody | TransportErrorKind::Other => true,
            },
            _ => false,
        }
    }

    /// Returns true if this error indicates the data doesn't exist.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::DataNotFound | Self::DataNotFoundFor { .. })
    }

    /// Returns true if this error is due to invalid input.
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            Self::InvalidCurrencyCode { .. } | Self::InvalidTickData | Self::InvalidRequest(_)
        )
    }

    /// Returns true if error is caused by client configuration.
    pub fn is_configuration_error(&self) -> bool {
        matches!(
            self,
            Self::MissingDefaultQuoteCurrency
                | Self::PairResolutionDisabled
                | Self::NoConversionRoute { .. }
        )
    }
}

impl From<reqwest::Error> for DukascopyError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            DukascopyError::Timeout(30)
        } else if err.is_connect() {
            DukascopyError::Transport {
                kind: TransportErrorKind::Connect,
                status: None,
                message: err.to_string(),
            }
        } else {
            DukascopyError::Transport {
                kind: TransportErrorKind::Other,
                status: err.status().map(|status| status.as_u16()),
                message: err.to_string(),
            }
        }
    }
}

impl From<lzma_rs::error::Error> for DukascopyError {
    fn from(err: lzma_rs::error::Error) -> Self {
        DukascopyError::LzmaError(err.to_string())
    }
}

impl From<io::Error> for DukascopyError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::TimedOut => DukascopyError::Timeout(30),
            io::ErrorKind::NotFound => DukascopyError::DataNotFound,
            _ => DukascopyError::Unknown(format!("IO error: {}", err)),
        }
    }
}

impl From<JoinError> for DukascopyError {
    fn from(err: JoinError) -> Self {
        if err.is_cancelled() {
            DukascopyError::Unknown("Task was cancelled".to_string())
        } else {
            DukascopyError::Unknown(format!("Task panicked: {}", err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable() {
        assert!(DukascopyError::RateLimitExceeded.is_retryable());
        assert!(DukascopyError::Timeout(30).is_retryable());
        assert!(DukascopyError::Transport {
            kind: TransportErrorKind::Connect,
            status: None,
            message: "connect".into()
        }
        .is_retryable());
        assert!(DukascopyError::Transport {
            kind: TransportErrorKind::HttpStatus,
            status: Some(503),
            message: "service unavailable".into()
        }
        .is_retryable());
        assert!(!DukascopyError::Transport {
            kind: TransportErrorKind::HttpStatus,
            status: Some(404),
            message: "not found".into()
        }
        .is_retryable());

        assert!(!DukascopyError::InvalidTickData.is_retryable());
        assert!(!DukascopyError::DataNotFound.is_retryable());
    }

    #[test]
    fn test_is_not_found() {
        assert!(DukascopyError::DataNotFound.is_not_found());
        assert!(DukascopyError::DataNotFoundFor {
            pair: "EUR/USD".into(),
            timestamp: "2024-01-01".into()
        }
        .is_not_found());

        assert!(!DukascopyError::InvalidTickData.is_not_found());
    }

    #[test]
    fn test_is_validation_error() {
        assert!(DukascopyError::InvalidTickData.is_validation_error());
        assert!(DukascopyError::InvalidCurrencyCode {
            code: "XX".into(),
            reason: "too short".into()
        }
        .is_validation_error());

        assert!(!DukascopyError::DataNotFound.is_validation_error());
    }

    #[test]
    fn test_is_configuration_error() {
        assert!(DukascopyError::MissingDefaultQuoteCurrency.is_configuration_error());
        assert!(DukascopyError::PairResolutionDisabled.is_configuration_error());
        assert!(DukascopyError::NoConversionRoute {
            symbol: "AAPL".into(),
            quote: "PLN".into()
        }
        .is_configuration_error());
        assert!(!DukascopyError::DataNotFound.is_configuration_error());
    }

    #[test]
    fn test_error_display() {
        let err = DukascopyError::InvalidCurrencyCode {
            code: "XX".into(),
            reason: "must be 3 characters".into(),
        };
        assert_eq!(
            err.to_string(),
            "Invalid currency code 'XX': must be 3 characters"
        );

        let err = DukascopyError::DataNotFoundFor {
            pair: "EUR/USD".into(),
            timestamp: "2024-01-01 12:00:00".into(),
        };
        assert!(err.to_string().contains("EUR/USD"));
        assert!(err.to_string().contains("2024-01-01"));
    }
}
