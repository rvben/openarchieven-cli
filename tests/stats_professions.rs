use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::stats::professions;
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
        quiet: false,
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
fn professions_returns_list_with_filters() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/professions.json"))
            .and(query_param("eventplace", "Leiden"))
            .and(query_param("eventyearstart", "1700"))
            .and(query_param("eventyearend", "1800"))
            .and(query_param("number_show", "20"))
            .and(query_param("lang", "nl"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "professions": [{"name": "smid", "count": 12}]
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = professions::run(
        &client,
        Some(&cache),
        &ctx(),
        &professions::Args {
            place: Some("Leiden".into()),
            year_start: Some(1700),
            year_end: Some(1800),
        },
    )
    .unwrap();

    assert_eq!(r.shape, Shape::List);
    assert_eq!(r.total, Some(1));
}

#[test]
fn professions_rejects_year_out_of_range() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = professions::run(
        &client,
        Some(&cache),
        &ctx(),
        &professions::Args {
            year_start: Some(1499),
            ..Default::default()
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year-start"));
}

#[test]
fn professions_rejects_unknown_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.lang = "es".into();

    let err =
        professions::run(&client, Some(&cache), &c, &professions::Args::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--lang"));
}

#[test]
fn professions_rejects_limit_over_100() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.limit = Some(101);

    let err =
        professions::run(&client, Some(&cache), &c, &professions::Args::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"));
}

#[test]
fn professions_rejects_zero_limit() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.limit = Some(0);

    let err =
        professions::run(&client, Some(&cache), &c, &professions::Args::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"), "msg: {}", err.message());
}

#[test]
fn professions_rejects_offset() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.offset = Some(10);

    let err =
        professions::run(&client, Some(&cache), &c, &professions::Args::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--offset"), "msg: {}", err.message());
}

#[test]
fn professions_rejects_year_end_out_of_range() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = professions::run(
        &client,
        Some(&cache),
        &ctx(),
        &professions::Args {
            year_end: Some(1961),
            ..Default::default()
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year-end"));
}

#[test]
fn professions_rejects_year_start_after_year_end() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = professions::run(
        &client,
        Some(&cache),
        &ctx(),
        &professions::Args {
            year_start: Some(1800),
            year_end: Some(1700),
            ..Default::default()
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--year-start"),
        "msg: {}",
        err.message()
    );
}
