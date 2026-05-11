use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::stats::breakdown;
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

fn args(group_by: &str) -> breakdown::Args {
    breakdown::Args {
        group_by: group_by.into(),
        ..Default::default()
    }
}

#[test]
fn breakdown_sends_all_filters_with_correct_wire_keys() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/breakdown.json"))
            .and(query_param("group_by", "archive"))
            .and(query_param("number_show", "100"))
            .and(query_param("lang", "nl"))
            .and(query_param("archive_code", "elo"))
            .and(query_param("sourcetype", "Bidprentjesverzameling"))
            .and(query_param("eventtype", "2"))
            .and(query_param("eventplace", "Amsterdam"))
            .and(query_param("year_start", "1700"))
            .and(query_param("year_end", "1800"))
            .and(query_param("min_count", "10"))
            .and(query_param("sort", "count_desc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "group_by": "archive",
                "filters": {"sourcetype": "Bidprentjesverzameling"},
                "total_records": 2991005,
                "total_groups": 47,
                "results": [{"key": "hwh", "label": "Heemkundekring Weerderheem", "count": 201236}]
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = breakdown::run(
        &client,
        Some(&cache),
        &ctx(),
        &breakdown::Args {
            group_by: "archive".into(),
            archive: Some("elo".into()),
            source_type: Some("Bidprentjesverzameling".into()),
            event_type: Some(2),
            place: Some("Amsterdam".into()),
            year_start: Some(1700),
            year_end: Some(1800),
            min_count: Some(10),
            sort: Some("count_desc".into()),
        },
    )
    .unwrap();

    assert_eq!(r.shape, Shape::SingleNested);
    assert_eq!(r.body["group_by"], "archive");
    assert_eq!(r.body["total_records"], 2991005);
    assert_eq!(r.body["total_groups"], 47);
    assert_eq!(r.body["results"][0]["key"], "hwh");
}

#[test]
fn breakdown_default_limit_is_100() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/breakdown.json"))
            .and(query_param("group_by", "year"))
            .and(query_param("number_show", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "group_by": "year",
                "total_records": 0,
                "total_groups": 0,
                "results": []
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = breakdown::run(&client, Some(&cache), &ctx(), &args("year")).unwrap();
    assert_eq!(r.shape, Shape::SingleNested);
}

#[test]
fn breakdown_rejects_unknown_group_by() {
    let rt = rt();
    let _server = rt.block_on(MockServer::start());
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let err = breakdown::run(&client, Some(&cache), &ctx(), &args("nonsense")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("group_by"));
}

#[test]
fn breakdown_rejects_limit_over_500() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut c = ctx();
    c.limit = Some(501);
    let err = breakdown::run(&client, Some(&cache), &c, &args("archive")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"));
}

#[test]
fn breakdown_rejects_zero_limit() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut c = ctx();
    c.limit = Some(0);
    let err = breakdown::run(&client, Some(&cache), &c, &args("archive")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"));
}

#[test]
fn breakdown_rejects_offset() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut c = ctx();
    c.offset = Some(5);
    let err = breakdown::run(&client, Some(&cache), &c, &args("archive")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--offset"));
}

#[test]
fn breakdown_rejects_unknown_sort() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut a = args("archive");
    a.sort = Some("bogus".into());
    let err = breakdown::run(&client, Some(&cache), &ctx(), &a).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--sort"));
}

#[test]
fn breakdown_rejects_unknown_lang() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut c = ctx();
    c.lang = "es".into();
    let err = breakdown::run(&client, Some(&cache), &c, &args("archive")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--lang"));
}

#[test]
fn breakdown_rejects_event_type_outside_enum() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut a = args("archive");
    a.event_type = Some(5);
    let err = breakdown::run(&client, Some(&cache), &ctx(), &a).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--event-type"));
}

#[test]
fn breakdown_rejects_year_start_below_range() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut a = args("archive");
    a.year_start = Some(1499);
    let err = breakdown::run(&client, Some(&cache), &ctx(), &a).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year-start"));
}

#[test]
fn breakdown_rejects_year_end_above_range() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut a = args("archive");
    a.year_end = Some(1961);
    let err = breakdown::run(&client, Some(&cache), &ctx(), &a).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year-end"));
}

#[test]
fn breakdown_rejects_year_start_after_year_end() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut a = args("archive");
    a.year_start = Some(1900);
    a.year_end = Some(1800);
    let err = breakdown::run(&client, Some(&cache), &ctx(), &a).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--year-start"));
}

#[test]
fn breakdown_rejects_zero_min_count() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".into(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut a = args("archive");
    a.min_count = Some(0);
    let err = breakdown::run(&client, Some(&cache), &ctx(), &a).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--min-count"));
}
