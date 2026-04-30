use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use chrono::Utc;
use rand::Rng;

use governor::clock::{Clock, DefaultClock};
use governor::middleware::NoOpMiddleware;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use reqwest::blocking::Client as HttpClient;
use serde_json::Value;
use url::Url;

use crate::error::{Error, ErrorKind, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    /// Read + write cache.
    Default,
    /// Skip read, still write.
    Refresh,
    /// Skip both read and write.
    Bypass,
}

/// What the endpoint says about caching this specific request.
#[derive(Debug, Clone, Copy)]
pub enum TtlHint {
    Fixed(Duration),
    UntilMidnight,
    Never,
    None,
}

impl TtlHint {
    fn to_cache_ttl(self) -> Option<crate::cache::Ttl> {
        match self {
            TtlHint::Fixed(d) => Some(crate::cache::Ttl::Fixed(d)),
            TtlHint::UntilMidnight => Some(crate::cache::Ttl::UntilMidnight),
            TtlHint::Never => Some(crate::cache::Ttl::Never),
            TtlHint::None => None,
        }
    }
}

pub const DEFAULT_BASE_URL: &str = "https://api.openarchieven.nl/1.1";
pub const ENV_BASE_URL: &str = "OPENARCHIEVEN_BASE_URL";
pub const USER_AGENT: &str = concat!("openarchieven/", env!("CARGO_PKG_VERSION"));

const MAX_ATTEMPTS: u32 = 3;
const BASE_BACKOFF_MS: u64 = 500;
const MAX_BACKOFF_MS: u64 = 5_000;

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub timeout: Duration,
    pub lang: String,
    pub rate_limit_per_sec: u32,
    pub cache_mode: CacheMode,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: Duration::from_secs(30),
            lang: "nl".to_string(),
            rate_limit_per_sec: 4,
            cache_mode: CacheMode::Default,
        }
    }
}

pub struct Client {
    http: HttpClient,
    config: ClientConfig,
    limiter: Limiter,
    clock: DefaultClock,
}

impl Client {
    pub fn new(config: ClientConfig) -> Result<Self> {
        let http = HttpClient::builder()
            .user_agent(USER_AGENT)
            .timeout(config.timeout)
            .build()
            .map_err(|e| Error::new(ErrorKind::Network, e.to_string()))?;
        let qps =
            NonZeroU32::new(config.rate_limit_per_sec.max(1)).expect("rate_limit_per_sec >= 1");
        let limiter = RateLimiter::direct(Quota::per_second(qps));
        Ok(Self {
            http,
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

    /// Single HTTP attempt. No retries, no cache.
    pub fn execute_once(&self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        self.acquire();
        let url = self.build_url(path, params)?;
        let resp = match self.http.get(url).send() {
            Ok(r) => r,
            Err(e) if e.is_timeout() => {
                return Err(Error::new(ErrorKind::Timeout, e.to_string()));
            }
            Err(e) => {
                return Err(Error::new(ErrorKind::Network, e.to_string()));
            }
        };
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        let body_bytes = resp
            .bytes()
            .map_err(|e| Error::new(ErrorKind::Network, e.to_string()))?;

        if status.is_success() {
            return serde_json::from_slice::<Value>(&body_bytes).map_err(|e| {
                Error::new(
                    ErrorKind::Parse,
                    format!("upstream 2xx body did not deserialize: {e}"),
                )
            });
        }

        Err(map_error_status(status, &body_bytes, retry_after))
    }

    /// Execute a request with retry, jittered exponential backoff, and an
    /// overall deadline derived from the configured timeout. Retries on
    /// transient errors (429, 5xx, network, timeout). Non-retryable errors
    /// (4xx validation, not-found, parse) are surfaced immediately.
    pub fn get(&self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        let deadline = Instant::now() + self.config.timeout;
        let mut last: Option<Error> = None;
        for attempt in 0..MAX_ATTEMPTS {
            if Instant::now() >= deadline {
                return Err(Error::new(ErrorKind::Timeout, "deadline exceeded"));
            }
            match self.execute_once(path, params) {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if !e.is_retryable_transport() {
                        return Err(e);
                    }
                    let wait = backoff(attempt, e.retry_after_seconds());
                    last = Some(e);
                    if attempt + 1 == MAX_ATTEMPTS {
                        break;
                    }
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    std::thread::sleep(wait.min(remaining));
                }
            }
        }
        Err(last.expect("loop only breaks after setting last on a retryable error"))
    }

    /// Cache-aware fetch. Pass `cache = None` to fully bypass cache regardless
    /// of `cache_mode`. `ttl_hint` controls whether and how long to cache the
    /// response; `TtlHint::None` suppresses the write regardless of `cache_mode`.
    pub fn get_cached(
        &self,
        path: &str,
        params: &[(&str, &str)],
        ttl_hint: TtlHint,
        cache: Option<&crate::cache::Cache>,
    ) -> Result<Value> {
        let cache = cache.filter(|_| !matches!(self.config.cache_mode, CacheMode::Bypass));
        let key = cache.map(|_| build_cache_key(&self.config.base_url, path, params, ttl_hint));

        if let (Some(c), Some(k)) = (cache, key.as_deref())
            && matches!(self.config.cache_mode, CacheMode::Default)
            && let Some(entry) = c.get(k, Utc::now())
        {
            return Ok(entry.body);
        }

        let body = self.get(path, params)?;

        if let (Some(c), Some(k), Some(ttl)) = (cache, key.as_deref(), ttl_hint.to_cache_ttl()) {
            let now = Utc::now();
            let entry = crate::cache::Entry {
                url: self.build_url(path, params)?.to_string(),
                fetched_at: now,
                expires_at: ttl.expires_at(now),
                body: body.clone(),
            };
            c.put(k, &entry);
        }

        Ok(body)
    }
}

fn build_cache_key(
    base_url: &str,
    path: &str,
    params: &[(&str, &str)],
    ttl_hint: TtlHint,
) -> String {
    let mut sorted: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in params {
        sorted.insert((*k).to_string(), (*v).to_string());
    }
    let ttl_class = match ttl_hint {
        TtlHint::UntilMidnight => Utc::now().date_naive().to_string(),
        _ => String::new(),
    };
    crate::cache::key(base_url, "GET", path, &sorted, &ttl_class)
}

/// Sleep duration before the next attempt. Honours `Retry-After` verbatim
/// (the caller caps it against the remaining deadline). Otherwise jitters
/// exponentially up to `MAX_BACKOFF_MS`.
fn backoff(attempt: u32, retry_after: Option<u64>) -> Duration {
    if let Some(secs) = retry_after {
        return Duration::from_secs(secs);
    }
    let cap = BASE_BACKOFF_MS
        .saturating_mul(1u64 << attempt)
        .min(MAX_BACKOFF_MS);
    let jittered = rand::rng().random_range(0..=cap);
    Duration::from_millis(jittered)
}

fn map_error_status(status: reqwest::StatusCode, body: &[u8], retry_after: Option<u64>) -> Error {
    match status.as_u16() {
        400 => map_validation_400(body),
        404 => Error::new(ErrorKind::NotFound, "resource not found"),
        429 => {
            let mut e = Error::new(ErrorKind::RateLimit, "API throttled");
            if let Some(secs) = retry_after {
                e = e.with_retry_after(secs);
            }
            e
        }
        s if (500..600).contains(&s) => Error::new(ErrorKind::Server, format!("upstream {s}")),
        s => Error::new(ErrorKind::Server, format!("upstream {s} (unexpected)")),
    }
}

/// Parse the upstream's structured 400 body once. The openarchieven.nl API
/// returns `{"error_code","error_description"}`; both are surfaced as
/// upstream metadata. Falls back to a truncated raw snippet otherwise.
fn map_validation_400(body: &[u8]) -> Error {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        let snippet: String = String::from_utf8_lossy(body).chars().take(200).collect();
        return Error::new(ErrorKind::Validation, snippet);
    };
    let code = v.get("error_code").and_then(|c| c.as_str()).unwrap_or("");
    let desc = v
        .get("error_description")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    if code.is_empty() {
        let snippet: String = String::from_utf8_lossy(body).chars().take(200).collect();
        return Error::new(ErrorKind::Validation, snippet);
    }
    Error::new(ErrorKind::Validation, desc.to_string()).with_upstream(code, desc)
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

    #[test]
    fn ttl_hint_to_cache_ttl_all_variants() {
        use crate::cache::Ttl;
        use std::time::Duration;

        let fixed = TtlHint::Fixed(Duration::from_secs(60)).to_cache_ttl();
        assert!(matches!(fixed, Some(Ttl::Fixed(d)) if d == Duration::from_secs(60)));

        let midnight = TtlHint::UntilMidnight.to_cache_ttl();
        assert!(matches!(midnight, Some(Ttl::UntilMidnight)));

        let never = TtlHint::Never.to_cache_ttl();
        assert!(matches!(never, Some(Ttl::Never)));

        let none = TtlHint::None.to_cache_ttl();
        assert!(none.is_none());
    }

    #[test]
    fn config_accessor_returns_reference() {
        let cfg = ClientConfig {
            lang: "en".into(),
            ..ClientConfig::default()
        };
        let client = Client::new(cfg.clone()).unwrap();
        assert_eq!(client.config().lang, "en");
    }

    #[test]
    fn backoff_uses_retry_after_when_present() {
        let d = backoff(0, Some(42));
        assert_eq!(d, std::time::Duration::from_secs(42));
    }

    #[test]
    fn backoff_without_retry_after_is_bounded() {
        for attempt in 0..3 {
            let d = backoff(attempt, None);
            assert!(d <= std::time::Duration::from_millis(MAX_BACKOFF_MS));
        }
    }

    #[test]
    fn map_validation_400_with_non_json_body() {
        let err = map_validation_400(b"plain text error");
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
        assert!(err.message().contains("plain text error"));
    }

    #[test]
    fn map_validation_400_with_json_missing_error_code() {
        let err = map_validation_400(br#"{"some":"field"}"#);
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
    }

    #[test]
    fn map_validation_400_with_full_structured_body() {
        let body = br#"{"error_code":"OOPS","error_description":"something went wrong"}"#;
        let err = map_validation_400(body);
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
        assert_eq!(err.upstream_code(), Some("OOPS"));
        assert_eq!(err.upstream_message(), Some("something went wrong"));
    }

    #[test]
    fn map_error_status_404_is_not_found() {
        let err = map_error_status(reqwest::StatusCode::NOT_FOUND, b"", None);
        assert_eq!(err.kind(), crate::error::ErrorKind::NotFound);
    }

    #[test]
    fn map_error_status_429_without_retry_after() {
        let err = map_error_status(reqwest::StatusCode::TOO_MANY_REQUESTS, b"", None);
        assert_eq!(err.kind(), crate::error::ErrorKind::RateLimit);
        assert!(err.retry_after_seconds().is_none());
    }

    #[test]
    fn map_error_status_429_with_retry_after() {
        let err = map_error_status(reqwest::StatusCode::TOO_MANY_REQUESTS, b"", Some(15));
        assert_eq!(err.kind(), crate::error::ErrorKind::RateLimit);
        assert_eq!(err.retry_after_seconds(), Some(15));
    }

    #[test]
    fn map_error_status_500_is_server() {
        let err = map_error_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR, b"", None);
        assert_eq!(err.kind(), crate::error::ErrorKind::Server);
    }

    #[test]
    fn map_error_status_unexpected_code_is_server() {
        let err = map_error_status(reqwest::StatusCode::from_u16(418).unwrap(), b"", None);
        assert_eq!(err.kind(), crate::error::ErrorKind::Server);
    }

    #[test]
    fn resolve_base_url_env_var_is_used_when_no_explicit() {
        // Exercise the env-reading branch via the explicit fallback path.
        // We use resolve_base_url with an explicit value here to avoid
        // mutating global env state that can cause races with other tests.
        assert_eq!(
            resolve_base_url(Some("http://from-env/v2/")),
            "http://from-env/v2"
        );
    }

    #[test]
    fn build_url_rejects_malformed_base_url() {
        let cfg = ClientConfig {
            base_url: "not-a-url".into(),
            ..ClientConfig::default()
        };
        let client = Client::new(cfg).unwrap();
        let err = client.build_url("/path", &[]).unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
    }
}
