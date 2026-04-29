use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use openarchieven::client::{Client, ClientConfig};
use openarchieven::error::ErrorKind;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn client(base_url: &str, timeout_ms: u64) -> Client {
    Client::new(ClientConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_millis(timeout_ms),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
    })
    .unwrap()
}

/// Returns 503 the first N times, then 200.
struct ThenOk {
    count: Arc<AtomicUsize>,
    fails_before_ok: usize,
}
impl Respond for ThenOk {
    fn respond(&self, _req: &wiremock::Request) -> ResponseTemplate {
        let n = self.count.fetch_add(1, Ordering::SeqCst);
        if n < self.fails_before_ok {
            ResponseTemplate::new(503)
        } else {
            ResponseTemplate::new(200).set_body_json(json!({"ok": true}))
        }
    }
}

#[test]
fn retries_on_503_then_succeeds() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    let counter = Arc::new(AtomicUsize::new(0));
    let mock_counter = counter.clone();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(ThenOk {
                count: mock_counter,
                fails_before_ok: 1,
            })
            .mount(&server)
            .await;
    });
    let c = client(&server.uri(), 5_000);
    let v = c.get("/x", &[]).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

#[test]
fn gives_up_after_three_attempts_and_returns_server() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    let counter = Arc::new(AtomicUsize::new(0));
    let mock_counter = counter.clone();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(ThenOk {
                count: mock_counter,
                fails_before_ok: 99,
            })
            .mount(&server)
            .await;
    });
    let c = client(&server.uri(), 5_000);
    let err = c.get("/x", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Server);
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[test]
fn retries_on_429_and_surfaces_rate_limit_after_exhaustion() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri(), 5_000);
    let err = c.get("/x", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimit);
    assert_eq!(err.retry_after_seconds(), Some(0));
}

#[test]
fn validation_400_does_not_retry() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    let counter = Arc::new(AtomicUsize::new(0));
    struct Count400(Arc<AtomicUsize>);
    impl Respond for Count400 {
        fn respond(&self, _req: &wiremock::Request) -> ResponseTemplate {
            self.0.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(400).set_body_json(json!({
                "error_code": "BAD",
                "error_description": "no"
            }))
        }
    }
    let mock_counter = counter.clone();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(Count400(mock_counter))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri(), 5_000);
    let err = c.get("/x", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}
