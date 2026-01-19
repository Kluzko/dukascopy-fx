//! HTTP client for fetching tick data from Dukascopy.

use crate::error::DukascopyError;
use lru::LruCache;
use reqwest::Client;
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::OnceCell;

/// Default LRU cache size for decompressed tick data
pub const DEFAULT_CACHE_SIZE: usize = 100;

/// Default maximum idle connections per host
pub const DEFAULT_MAX_IDLE_CONNECTIONS: usize = 10;

/// Default HTTP request timeout in seconds
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Dukascopy API base URL
pub const DUKASCOPY_BASE_URL: &str = "https://datafeed.dukascopy.com/datafeed";

static CLIENT: OnceCell<Client> = OnceCell::const_new();
static CACHE: OnceCell<Mutex<LruCache<String, Vec<u8>>>> = OnceCell::const_new();

/// Gets or initializes the global cache
async fn get_cache() -> &'static Mutex<LruCache<String, Vec<u8>>> {
    CACHE
        .get_or_init(|| async {
            Mutex::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_CACHE_SIZE).expect("Cache size must be non-zero"),
            ))
        })
        .await
}

/// HTTP client for fetching and caching Dukascopy tick data.
///
/// This client uses a global singleton pattern with LRU caching for efficiency.
/// All methods are static as the client maintains global state.
///
/// # Example
///
/// ```no_run
/// use dukascopy_fx::DukascopyClient;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let url = DukascopyClient::build_url("EURUSD", 2024, 1, 15, 14);
/// let data = DukascopyClient::get_cached_data(&url).await?;
/// # Ok(())
/// # }
/// ```
pub struct DukascopyClient;

impl DukascopyClient {
    /// Get or create the global HTTP client.
    #[inline]
    pub async fn get_client() -> &'static Client {
        CLIENT
            .get_or_init(|| async {
                Client::builder()
                    .pool_max_idle_per_host(DEFAULT_MAX_IDLE_CONNECTIONS)
                    .tcp_nodelay(true)
                    .pool_idle_timeout(None)
                    .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
                    .build()
                    .expect("Failed to create HTTP client")
            })
            .await
    }

    /// Get cached data or fetch from URL.
    ///
    /// This method:
    /// 1. Checks the LRU cache for existing data
    /// 2. If not cached, fetches from the URL
    /// 3. Decompresses the LZMA data
    /// 4. Caches and returns the decompressed data
    ///
    /// # Arguments
    /// * `url` - The URL to fetch tick data from
    ///
    /// # Returns
    /// Decompressed tick data bytes
    ///
    /// # Errors
    /// - `HttpError` - Network or HTTP errors
    /// - `LzmaError` - Decompression failed
    /// - `DataNotFound` - HTTP 404
    /// - `RateLimitExceeded` - HTTP 429
    pub async fn get_cached_data(url: &str) -> Result<Vec<u8>, DukascopyError> {
        let cache = get_cache().await;

        // Check cache first
        {
            let mut cache_guard = cache
                .lock()
                .map_err(|e| DukascopyError::CacheError(format!("Cache lock poisoned: {}", e)))?;

            if let Some(data) = cache_guard.get(url) {
                return Ok(data.clone());
            }
        }

        // Fetch from network
        let client = Self::get_client().await;
        let response = client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(Self::map_http_error(response.status()));
        }

        let bytes = response.bytes().await?;

        // Validate we have data before decompressing
        if bytes.is_empty() {
            return Err(DukascopyError::DataNotFound);
        }

        // Decompress in blocking task to not block async runtime
        let decompressed_data = tokio::task::spawn_blocking(move || {
            let mut decompressed = Vec::with_capacity(bytes.len() * 4);
            lzma_rs::lzma_decompress(&mut Cursor::new(&bytes), &mut decompressed)?;
            Ok::<_, DukascopyError>(decompressed)
        })
        .await??;

        // Validate decompressed data is not empty
        if decompressed_data.is_empty() {
            return Err(DukascopyError::DataNotFound);
        }

        // Cache the result
        {
            let mut cache_guard = cache
                .lock()
                .map_err(|e| DukascopyError::CacheError(format!("Cache lock poisoned: {}", e)))?;

            cache_guard.put(url.to_string(), decompressed_data.clone());
        }

        Ok(decompressed_data)
    }

    /// Maps HTTP status codes to DukascopyError
    fn map_http_error(status: reqwest::StatusCode) -> DukascopyError {
        match status {
            reqwest::StatusCode::NOT_FOUND => DukascopyError::DataNotFound,
            reqwest::StatusCode::TOO_MANY_REQUESTS => DukascopyError::RateLimitExceeded,
            reqwest::StatusCode::UNAUTHORIZED => DukascopyError::Unauthorized,
            reqwest::StatusCode::FORBIDDEN => DukascopyError::Forbidden,
            reqwest::StatusCode::BAD_REQUEST => {
                DukascopyError::InvalidRequest("Bad request".to_string())
            }
            status => DukascopyError::HttpError(format!(
                "HTTP {} - {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            )),
        }
    }

    /// Build a URL for fetching tick data.
    ///
    /// # Arguments
    /// * `pair_symbol` - Combined currency pair (e.g., "EURUSD")
    /// * `year` - Year
    /// * `month` - Month (1-12, will be converted to 0-indexed)
    /// * `day` - Day of month
    /// * `hour` - Hour (0-23)
    ///
    /// # Returns
    /// The complete URL for the tick data file
    pub fn build_url(pair_symbol: &str, year: i32, month: u32, day: u32, hour: u32) -> String {
        format!(
            "{}/{}/{}/{:02}/{:02}/{}h_ticks.bi5",
            DUKASCOPY_BASE_URL,
            pair_symbol,
            year,
            month - 1, // Dukascopy uses 0-indexed months
            day,
            hour
        )
    }

    /// Clear the cache.
    ///
    /// This can be useful for testing or when you want to force fresh data.
    pub async fn clear_cache() -> Result<(), DukascopyError> {
        let cache = get_cache().await;
        let mut cache_guard = cache
            .lock()
            .map_err(|e| DukascopyError::CacheError(format!("Cache lock poisoned: {}", e)))?;

        cache_guard.clear();
        Ok(())
    }

    /// Get the current number of cached entries.
    pub async fn cache_len() -> Result<usize, DukascopyError> {
        let cache = get_cache().await;
        let cache_guard = cache
            .lock()
            .map_err(|e| DukascopyError::CacheError(format!("Cache lock poisoned: {}", e)))?;

        Ok(cache_guard.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_url() {
        let url = DukascopyClient::build_url("EURUSD", 2024, 1, 15, 14);
        assert_eq!(
            url,
            "https://datafeed.dukascopy.com/datafeed/EURUSD/2024/00/15/14h_ticks.bi5"
        );
    }

    #[test]
    fn test_build_url_december() {
        let url = DukascopyClient::build_url("USDJPY", 2024, 12, 31, 23);
        assert_eq!(
            url,
            "https://datafeed.dukascopy.com/datafeed/USDJPY/2024/11/31/23h_ticks.bi5"
        );
    }

    #[test]
    fn test_build_url_edge_cases() {
        // First day of year
        let url = DukascopyClient::build_url("EURUSD", 2024, 1, 1, 0);
        assert!(url.contains("/2024/00/01/0h_ticks.bi5"));

        // Last day of year
        let url = DukascopyClient::build_url("EURUSD", 2024, 12, 31, 23);
        assert!(url.contains("/2024/11/31/23h_ticks.bi5"));
    }

    #[test]
    fn test_map_http_error() {
        assert!(matches!(
            DukascopyClient::map_http_error(reqwest::StatusCode::NOT_FOUND),
            DukascopyError::DataNotFound
        ));
        assert!(matches!(
            DukascopyClient::map_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS),
            DukascopyError::RateLimitExceeded
        ));
        assert!(matches!(
            DukascopyClient::map_http_error(reqwest::StatusCode::UNAUTHORIZED),
            DukascopyError::Unauthorized
        ));
    }
}
