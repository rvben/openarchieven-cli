use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::search;
use openarchieven::error::ErrorKind;
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

fn client(server: &MockServer) -> Client {
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
fn search_passes_pagination_and_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search.json"))
            .and(query_param("name", "jansen"))
            .and(query_param("number_show", "5"))
            .and(query_param("start", "10"))
            .and(query_param("lang", "en"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {
                    "numFound": 100,
                    "docs": [{"id": "rec-1"}, {"id": "rec-2"}]
                }
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.limit = Some(5);
    ctx.offset = Some(10);
    ctx.lang = "en".into();

    let args = search::Args {
        name: "jansen".into(),
        archive: None,
        source_type: None,
        event_place: None,
        birth_place: None,
        relation_type: None,
        country: None,
        sort: None,
    };

    let r = search::run(&client, Some(&cache), &ctx, &args).unwrap();
    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["paginated"], true);
    assert_eq!(envelope["total"], 100);
    assert_eq!(envelope["limit"], 5);
    assert_eq!(envelope["offset"], 10);
    assert_eq!(envelope["items"].as_array().unwrap().len(), 2);
}

#[test]
fn search_rejects_unknown_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.lang = "de".into();

    let args = search::Args {
        name: "jansen".into(),
        archive: None,
        source_type: None,
        event_place: None,
        birth_place: None,
        relation_type: None,
        country: None,
        sort: None,
    };

    let err = search::run(&client, Some(&cache), &ctx, &args).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--lang"),
        "message was: {}",
        err.message()
    );
}

#[test]
fn search_caps_limit_at_100() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.limit = Some(200);

    let args = search::Args {
        name: "jansen".into(),
        archive: None,
        source_type: None,
        event_place: None,
        birth_place: None,
        relation_type: None,
        country: None,
        sort: None,
    };

    let err = search::run(&client, Some(&cache), &ctx, &args).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("100"),
        "message was: {}",
        err.message()
    );
}

#[test]
fn search_rejects_zero_limit() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.limit = Some(0);

    let args = search::Args {
        name: "jansen".into(),
        ..Default::default()
    };

    let err = search::run(&client, Some(&cache), &ctx, &args).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("at least 1"),
        "msg: {}",
        err.message()
    );
}

#[test]
fn search_sends_optional_filters() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search.json"))
            .and(query_param("name", "jansen"))
            .and(query_param("archive_code", "elo"))
            .and(query_param("sourcetype", "BS"))
            .and(query_param("eventplace", "Amsterdam"))
            .and(query_param("birthplace", "Leiden"))
            .and(query_param("relationtype", "vader"))
            .and(query_param("country_code", "NL"))
            .and(query_param("sort", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"numFound": 1, "docs": [{"id": "x"}]}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let args = search::Args {
        name: "jansen".into(),
        archive: Some("elo".into()),
        source_type: Some("BS".into()),
        event_place: Some("Amsterdam".into()),
        birth_place: Some("Leiden".into()),
        relation_type: Some("vader".into()),
        country: Some("NL".into()),
        sort: Some(2),
    };

    let r = search::run(&client, Some(&cache), &ctx(), &args).unwrap();
    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["total"], 1);
}

#[test]
fn search_handles_missing_response_docs() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let args = search::Args {
        name: "jansen".into(),
        ..Default::default()
    };

    let r = search::run(&client, Some(&cache), &ctx(), &args).unwrap();
    assert!(r.total.is_none());
    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["items"].as_array().unwrap().len(), 0);
}
