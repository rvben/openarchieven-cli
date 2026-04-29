use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::stats::archive_stat::ArchiveStatArgs;
use openarchieven::commands::stats::{comments, events, records, sources};
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
fn stats_records_returns_list_with_total() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/records.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "records": [
                    {"archive": "elo", "count": 1000},
                    {"archive": "saa", "count": 5000}
                ]
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = records::run(&client, Some(&cache), &ctx(), &ArchiveStatArgs::default()).unwrap();

    assert_eq!(r.shape, Shape::List);
    assert_eq!(r.total, Some(2));
    let env = r.list_envelope(r.total);
    assert_eq!(env["total"], 2);
    assert_eq!(env["paginated"], false);
    assert_eq!(env["items"][0]["archive"], "elo");
}

#[test]
fn stats_sources_filters_by_archive() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/sources.json"))
            .and(query_param("archive_code", "elo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sources": []})))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = sources::run(
        &client,
        Some(&cache),
        &ctx(),
        &ArchiveStatArgs {
            archive: Some("elo".into()),
        },
    )
    .unwrap();

    assert_eq!(r.shape, Shape::List);
    assert_eq!(r.total, Some(0));
}

#[test]
fn stats_events_basic() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/events.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "events": [{"e": "birth"}, {"e": "death"}, {"e": "marriage"}]
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = events::run(&client, Some(&cache), &ctx(), &ArchiveStatArgs::default()).unwrap();

    assert_eq!(r.total, Some(3));
}

#[test]
fn stats_comments_basic() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/comments.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"comments": [{"c": 1}]})))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = comments::run(&client, Some(&cache), &ctx(), &ArchiveStatArgs::default()).unwrap();

    assert_eq!(r.total, Some(1));
}

#[test]
fn stats_records_rejects_limit() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.limit = Some(50);

    let err = records::run(&client, Some(&cache), &c, &ArchiveStatArgs::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit") || err.message().contains("--offset"));
}

#[test]
fn stats_records_rejects_lang_other_than_nl() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.lang = "en".into();

    let err = records::run(&client, Some(&cache), &c, &ArchiveStatArgs::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--lang"));
}

#[test]
fn stats_records_rejects_fields() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.fields = Some(vec!["records".into()]);

    let err = records::run(&client, Some(&cache), &c, &ArchiveStatArgs::default()).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--fields"));
}
