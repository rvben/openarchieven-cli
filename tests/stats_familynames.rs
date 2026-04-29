use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::stats::familynames;
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
fn familynames_paginates_with_filters() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/familynames.json"))
            .and(query_param("place", "Leiden"))
            .and(query_param("year_start", "1700"))
            .and(query_param("year_end", "1800"))
            .and(query_param("event_type", "1"))
            .and(query_param("number_show", "20"))
            .and(query_param("lang", "en"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "familynames": [{"name": "Jansen", "count": 42}]
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.limit = Some(20);
    c.lang = "en".into();

    let r = familynames::run(
        &client,
        Some(&cache),
        &c,
        &familynames::Args {
            place: Some("Leiden".into()),
            year_start: Some(1700),
            year_end: Some(1800),
            event_type: Some(1),
        },
    )
    .unwrap();

    assert_eq!(r.shape, Shape::List);
    assert_eq!(r.total, Some(1));
    assert_eq!(r.limit, Some(20));
}

#[test]
fn familynames_default_limit_is_20() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/familynames.json"))
            .and(query_param("number_show", "20"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"familynames": []})))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = familynames::run(&client, Some(&cache), &ctx(), &familynames::Args::default()).unwrap();
    assert_eq!(r.limit, Some(20));
}

#[test]
fn familynames_rejects_year_start_below_range() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = familynames::run(
        &client,
        Some(&cache),
        &ctx(),
        &familynames::Args {
            year_start: Some(1499),
            ..Default::default()
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year-start"));
}

#[test]
fn familynames_rejects_year_end_above_range() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = familynames::run(
        &client,
        Some(&cache),
        &ctx(),
        &familynames::Args {
            year_end: Some(1961),
            ..Default::default()
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year-end"));
}

#[test]
fn familynames_rejects_unknown_event_type() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = familynames::run(
        &client,
        Some(&cache),
        &ctx(),
        &familynames::Args {
            event_type: Some(4),
            ..Default::default()
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--event-type"));
}

#[test]
fn familynames_rejects_year_start_after_end() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = familynames::run(
        &client,
        Some(&cache),
        &ctx(),
        &familynames::Args {
            year_start: Some(1800),
            year_end: Some(1700),
            ..Default::default()
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year-start"));
}

#[test]
fn familynames_rejects_limit_over_100() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.limit = Some(101);

    let err =
        familynames::run(&client, Some(&cache), &c, &familynames::Args::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"));
}

#[test]
fn familynames_rejects_offset() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.offset = Some(5);

    let err =
        familynames::run(&client, Some(&cache), &c, &familynames::Args::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--offset"));
}

#[test]
fn familynames_rejects_unknown_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.lang = "es".into();

    let err =
        familynames::run(&client, Some(&cache), &c, &familynames::Args::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--lang"));
}
