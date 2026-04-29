use std::num::NonZeroU32;
use std::time::Duration;

use governor::clock::{Clock, DefaultClock};
use governor::middleware::NoOpMiddleware;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use url::Url;

use crate::error::{Error, ErrorKind, Result};

pub const DEFAULT_BASE_URL: &str = "https://api.openarchieven.nl/1.1";
pub const ENV_BASE_URL: &str = "OPENARCHIEVEN_BASE_URL";
pub const USER_AGENT: &str = concat!("openarchieven/", env!("CARGO_PKG_VERSION"));

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub timeout: Duration,
    pub lang: String,
    pub rate_limit_per_sec: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: Duration::from_secs(30),
            lang: "nl".to_string(),
            rate_limit_per_sec: 4,
        }
    }
}

pub struct Client {
    config: ClientConfig,
    limiter: Limiter,
    clock: DefaultClock,
}

impl Client {
    pub fn new(config: ClientConfig) -> Result<Self> {
        let qps =
            NonZeroU32::new(config.rate_limit_per_sec.max(1)).expect("rate_limit_per_sec >= 1");
        let limiter = RateLimiter::direct(Quota::per_second(qps));
        Ok(Self {
            config,
            limiter,
            clock: DefaultClock::default(),
        })
    }

    /// Block (synchronously) until a rate-limit token is available.
    pub fn acquire(&self) {
        loop {
            match self.limiter.check() {
                Ok(_) => return,
                Err(not_until) => {
                    let wait = not_until.wait_time_from(self.clock.now());
                    std::thread::sleep(wait);
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn acquire_for_test(&self) {
        self.acquire();
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Build a fully-resolved URL for an API path with sorted query params.
    /// `path` is a leading-slash path (e.g. `/records/search`). Params are
    /// percent-encoded by `url::Url`.
    pub fn build_url(&self, path: &str, params: &[(&str, &str)]) -> Result<Url> {
        let base = self.config.base_url.trim_end_matches('/');
        let joined = format!("{base}{path}");
        let mut url = Url::parse(&joined)
            .map_err(|e| Error::new(ErrorKind::Validation, format!("bad url: {e}")))?;
        if !params.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in params {
                pairs.append_pair(k, v);
            }
        }
        Ok(url)
    }
}

/// Resolve the API base URL: explicit > env > default. The returned string
/// is trimmed of trailing slashes so it composes cleanly with leading-slash
/// paths.
pub fn resolve_base_url(explicit: Option<&str>) -> String {
    let raw = explicit
        .map(str::to_owned)
        .or_else(|| std::env::var(ENV_BASE_URL).ok())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    raw.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Client {
        Client::new(ClientConfig::default()).unwrap()
    }

    #[test]
    fn build_url_no_params() {
        let url = client().build_url("/records/search", &[]).unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.openarchieven.nl/1.1/records/search"
        );
    }

    #[test]
    fn build_url_appends_query() {
        let url = client()
            .build_url("/records/search", &[("name", "jansen"), ("lang", "nl")])
            .unwrap();
        assert!(url.as_str().contains("name=jansen"));
        assert!(url.as_str().contains("lang=nl"));
    }

    #[test]
    fn build_url_percent_encodes() {
        let url = client()
            .build_url("/records/search", &[("name", "van der berg")])
            .unwrap();
        assert!(
            url.as_str().contains("name=van+der+berg")
                || url.as_str().contains("name=van%20der%20berg")
        );
    }

    #[test]
    fn resolve_base_url_explicit_wins() {
        // Use a unique env var so this test is order-independent.
        let prev = std::env::var(ENV_BASE_URL).ok();
        // SAFETY: tests are single-threaded for env mutation in this crate.
        unsafe {
            std::env::set_var(ENV_BASE_URL, "http://from-env");
        }
        assert_eq!(
            resolve_base_url(Some("http://explicit/")),
            "http://explicit"
        );
        match prev {
            Some(v) => unsafe { std::env::set_var(ENV_BASE_URL, v) },
            None => unsafe { std::env::remove_var(ENV_BASE_URL) },
        }
    }

    #[test]
    fn resolve_base_url_default_when_unset() {
        let prev = std::env::var(ENV_BASE_URL).ok();
        unsafe {
            std::env::remove_var(ENV_BASE_URL);
        }
        assert_eq!(resolve_base_url(None), DEFAULT_BASE_URL);
        if let Some(v) = prev {
            unsafe { std::env::set_var(ENV_BASE_URL, v) }
        }
    }

    #[test]
    fn resolve_base_url_strips_trailing_slash() {
        assert_eq!(
            resolve_base_url(Some("https://api.example.com/v1/")),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn rate_limiter_admits_first_token_immediately() {
        let cfg = ClientConfig {
            rate_limit_per_sec: 4,
            ..ClientConfig::default()
        };
        let client = Client::new(cfg).unwrap();
        let start = std::time::Instant::now();
        client.acquire_for_test();
        assert!(start.elapsed() < std::time::Duration::from_millis(50));
    }

    #[test]
    fn rate_limiter_throttles_burst() {
        // 2 req/sec → 5 requests should take at least ~1.5s (3 of them throttled).
        let cfg = ClientConfig {
            rate_limit_per_sec: 2,
            ..ClientConfig::default()
        };
        let client = Client::new(cfg).unwrap();
        let start = std::time::Instant::now();
        for _ in 0..5 {
            client.acquire_for_test();
        }
        assert!(
            start.elapsed() >= std::time::Duration::from_millis(1500),
            "expected throttling, took {:?}",
            start.elapsed()
        );
    }
}
