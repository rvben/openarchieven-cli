use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::yearsago;
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
fn yearsago_sends_years_and_limit() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/yearsago.json"))
            .and(query_param("years", "100"))
            .and(query_param("number_show", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"docs": [{"id": "y1"}]}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.limit = Some(5);

    let r = yearsago::run(&client, Some(&cache), &c, &yearsago::Args { years: 100 }).unwrap();

    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["paginated"], false);
    assert_eq!(envelope["total"], 1);
    assert_eq!(envelope["items"].as_array().unwrap().len(), 1);
}

#[test]
fn yearsago_rejects_offset() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.offset = Some(10);

    let err = yearsago::run(&client, Some(&cache), &c, &yearsago::Args { years: 100 }).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--offset"),
        "message: {}",
        err.message()
    );
}

#[test]
fn yearsago_rejects_lang_other_than_nl() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.lang = "en".into();

    let err = yearsago::run(&client, Some(&cache), &c, &yearsago::Args { years: 100 }).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--lang"),
        "message: {}",
        err.message()
    );
}

#[test]
fn yearsago_rejects_limit_over_100() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.limit = Some(200);

    let err = yearsago::run(&client, Some(&cache), &c, &yearsago::Args { years: 100 }).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"));
}

#[test]
fn yearsago_default_limit_is_10() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/yearsago.json"))
            .and(query_param("years", "50"))
            .and(query_param("number_show", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"docs": []}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = yearsago::run(&client, Some(&cache), &ctx(), &yearsago::Args { years: 50 }).unwrap();
    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["total"], 0);
}
