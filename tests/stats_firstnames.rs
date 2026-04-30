use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::stats::firstnames;
use openarchieven::error::ErrorKind;
use openarchieven::output::Shape;
use openarchieven::runtime::ApiContext;
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn ctx() -> ApiContext {
    ApiContext {
        timeout: Duration::from_secs(2),
        cache_mode: CacheMode::Default,
        cache_ttl_override: None,
        cache_dir: None,
        fields: None,
        limit: None,
        offset: None,
        lang: "nl".into(),
    }
}

fn make_client(server: &MockServer) -> Client {
    Client::new(ClientConfig {
        base_url: server.uri(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap()
}

#[test]
fn firstnames_returns_list_with_required_args() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/firstnames.json"))
            .and(query_param("place", "Leiden"))
            .and(query_param("eventyear", "1850"))
            .and(query_param("number_show", "20"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "firstnames": [{"name": "Jan", "count": 100}]
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = firstnames::run(
        &client,
        Some(&cache),
        &ctx(),
        &firstnames::Args {
            place: "Leiden".into(),
            year: 1850,
        },
    )
    .unwrap();

    assert_eq!(r.shape, Shape::List);
    assert_eq!(r.total, Some(1));
    assert_eq!(r.limit, Some(20));
}

#[test]
fn firstnames_rejects_year_below_range() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = firstnames::run(
        &client,
        Some(&cache),
        &ctx(),
        &firstnames::Args {
            place: "Leiden".into(),
            year: 1599,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year"));
}

#[test]
fn firstnames_rejects_lang_other_than_nl() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.lang = "en".into();

    let err = firstnames::run(
        &client,
        Some(&cache),
        &c,
        &firstnames::Args {
            place: "Leiden".into(),
            year: 1850,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--lang"));
}

#[test]
fn firstnames_rejects_offset() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.offset = Some(5);

    let err = firstnames::run(
        &client,
        Some(&cache),
        &c,
        &firstnames::Args {
            place: "Leiden".into(),
            year: 1850,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--offset"));
}

#[test]
fn firstnames_rejects_year_above_range() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = firstnames::run(
        &client,
        Some(&cache),
        &ctx(),
        &firstnames::Args {
            place: "Leiden".into(),
            year: 1961,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year"));
}

#[test]
fn firstnames_rejects_zero_limit() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.limit = Some(0);

    let err = firstnames::run(
        &client,
        Some(&cache),
        &c,
        &firstnames::Args {
            place: "Leiden".into(),
            year: 1850,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"));
}

#[test]
fn firstnames_rejects_limit_over_100() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.limit = Some(101);

    let err = firstnames::run(
        &client,
        Some(&cache),
        &c,
        &firstnames::Args {
            place: "Leiden".into(),
            year: 1850,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"));
}
