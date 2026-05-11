use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::show;
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
fn show_returns_nested_object() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/show.json"))
            .and(query_param("archive", "elo"))
            .and(query_param("identifier", "abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "record": {"id": "abc", "person": {"name": "Jan Jansen"}}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let r = show::run(
        &client,
        Some(&cache),
        &ctx(),
        &show::Args {
            archive: "elo".into(),
            identifier: "abc".into(),
        },
    )
    .unwrap();

    assert_eq!(r.shape, openarchieven::output::Shape::SingleNested);
    assert_eq!(r.body["record"]["id"], "abc");
    assert_eq!(r.body["record"]["person"]["name"], "Jan Jansen");
}

#[test]
fn show_404_is_not_found() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/show.json"))
            .and(query_param("archive", "elo"))
            .and(query_param("identifier", "missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let err = show::run(
        &client,
        Some(&cache),
        &ctx(),
        &show::Args {
            archive: "elo".into(),
            identifier: "missing".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[test]
fn show_empty_body_is_not_found() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/show.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let err = show::run(
        &client,
        Some(&cache),
        &ctx(),
        &show::Args {
            archive: "elo".into(),
            identifier: "ghost".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert!(
        err.message().contains("elo"),
        "message was: {}",
        err.message()
    );
    assert!(
        err.message().contains("ghost"),
        "message was: {}",
        err.message()
    );
}

#[test]
fn show_rejects_unknown_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.lang = "de".into();

    let err = show::run(
        &client,
        Some(&cache),
        &ctx,
        &show::Args {
            archive: "elo".into(),
            identifier: "abc".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--lang"), "msg: {}", err.message());
}

#[test]
fn show_rejects_limit_or_offset() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.offset = Some(5);

    let err = show::run(
        &client,
        Some(&cache),
        &ctx,
        &show::Args {
            archive: "elo".into(),
            identifier: "abc".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"), "msg: {}", err.message());
}

#[test]
fn show_null_body_is_not_found() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/show.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::Value::Null))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let err = show::run(
        &client,
        Some(&cache),
        &ctx(),
        &show::Args {
            archive: "elo".into(),
            identifier: "ghost".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[test]
fn show_upstream_invalid_archive_is_not_found() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/show.json"))
            .and(query_param("archive", "ZZZ"))
            .and(query_param("identifier", "12345"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "error_code": 1,
                "error_description": "Invalid archive",
                "request": "show"
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let err = show::run(
        &client,
        Some(&cache),
        &ctx(),
        &show::Args {
            archive: "ZZZ".into(),
            identifier: "12345".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(
        err.message(),
        "no record found for ZZZ/12345 (upstream: Invalid archive)"
    );
}
