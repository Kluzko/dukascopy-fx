use crate::error::DukascopyError;
use lru::LruCache;
use reqwest::Client;
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use tokio::sync::OnceCell;

static CLIENT: OnceCell<Client> = OnceCell::const_new();
static CACHE: OnceCell<Mutex<LruCache<String, Vec<u8>>>> = OnceCell::const_new();

pub struct DukascopyClient;

impl DukascopyClient {
    #[inline(always)]
    pub async fn get_client() -> &'static Client {
        CLIENT
            .get_or_init(|| async {
                Client::builder()
                    .pool_max_idle_per_host(10)
                    .tcp_nodelay(true)
                    .pool_idle_timeout(None)
                    .build()
                    .unwrap()
            })
            .await
    }

    pub async fn get_cached_data(url: &str) -> Result<Vec<u8>, DukascopyError> {
        let cache = CACHE
            .get_or_init(|| async { Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap())) })
            .await;

        {
            let mut cache = cache.lock().unwrap();
            if let Some(data) = cache.get(url) {
                return Ok(data.clone());
            }
        }

        let client = Self::get_client().await;
        let response = client.get(url).send().await?;

        if !response.status().is_success() {
            return match response.status() {
                reqwest::StatusCode::NOT_FOUND => Err(DukascopyError::DataNotFound),
                reqwest::StatusCode::TOO_MANY_REQUESTS => Err(DukascopyError::RateLimitExceeded),
                reqwest::StatusCode::UNAUTHORIZED => Err(DukascopyError::Unauthorized),
                reqwest::StatusCode::FORBIDDEN => Err(DukascopyError::Forbidden),
                reqwest::StatusCode::BAD_REQUEST => Err(DukascopyError::InvalidRequest),
                _ => Err(DukascopyError::HttpError(response.status().to_string())),
            };
        }

        let bytes = response.bytes().await?;

        let decompressed_data = tokio::task::spawn_blocking(move || {
            let mut decompressed = Vec::with_capacity(bytes.len() * 4);
            lzma_rs::lzma_decompress(&mut Cursor::new(&bytes), &mut decompressed)?;
            Ok::<_, DukascopyError>(decompressed)
        })
        .await??;

        let mut cache = cache.lock().unwrap();
        cache.put(url.to_string(), decompressed_data.clone());
        Ok(decompressed_data)
    }
}
