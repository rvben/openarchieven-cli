use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig, TtlHint};
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

struct Counted(Arc<AtomicUsize>);
impl Respond for Counted {
    fn respond(&self, _req: &wiremock::Request) -> ResponseTemplate {
        self.0.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(200).set_body_json(json!({"hit": true}))
    }
}

fn client(base_url: &str, mode: CacheMode) -> Client {
    Client::new(ClientConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: mode,
    })
    .unwrap()
}

fn mount_counted(rt: &tokio::runtime::Runtime, server: &MockServer, count: Arc<AtomicUsize>) {
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(Counted(count))
            .mount(server)
            .await;
    });
}

#[test]
fn second_call_is_served_from_cache() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    let count = Arc::new(AtomicUsize::new(0));
    mount_counted(&rt, &server, count.clone());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let c = client(&server.uri(), CacheMode::Default);

    c.get_cached(
        "/x",
        &[],
        TtlHint::Fixed(Duration::from_secs(60)),
        Some(&cache),
    )
    .unwrap();
    c.get_cached(
        "/x",
        &[],
        TtlHint::Fixed(Duration::from_secs(60)),
        Some(&cache),
    )
    .unwrap();
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "second call should hit cache"
    );
}

#[test]
fn bypass_mode_always_calls_upstream() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    let count = Arc::new(AtomicUsize::new(0));
    mount_counted(&rt, &server, count.clone());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let c = client(&server.uri(), CacheMode::Bypass);

    c.get_cached(
        "/x",
        &[],
        TtlHint::Fixed(Duration::from_secs(60)),
        Some(&cache),
    )
    .unwrap();
    c.get_cached(
        "/x",
        &[],
        TtlHint::Fixed(Duration::from_secs(60)),
        Some(&cache),
    )
    .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn refresh_mode_skips_read_but_writes() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    let count = Arc::new(AtomicUsize::new(0));
    mount_counted(&rt, &server, count.clone());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();

    let primer = client(&server.uri(), CacheMode::Default);
    primer
        .get_cached(
            "/x",
            &[],
            TtlHint::Fixed(Duration::from_secs(60)),
            Some(&cache),
        )
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);

    let refresher = client(&server.uri(), CacheMode::Refresh);
    refresher
        .get_cached(
            "/x",
            &[],
            TtlHint::Fixed(Duration::from_secs(60)),
            Some(&cache),
        )
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);

    let reader = client(&server.uri(), CacheMode::Default);
    reader
        .get_cached(
            "/x",
            &[],
            TtlHint::Fixed(Duration::from_secs(60)),
            Some(&cache),
        )
        .unwrap();
    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "refresh should have written a fresh entry"
    );
}

#[test]
fn ttl_hint_none_does_not_write() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    let count = Arc::new(AtomicUsize::new(0));
    mount_counted(&rt, &server, count.clone());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let c = client(&server.uri(), CacheMode::Default);

    c.get_cached("/x", &[], TtlHint::None, Some(&cache))
        .unwrap();
    c.get_cached("/x", &[], TtlHint::None, Some(&cache))
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
