use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::census;
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
fn census_returns_nested() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/related/census.json"))
            .and(query_param("year", "1850"))
            .and(query_param("place", "Leiden"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "census": {"year": 1850, "tables": []}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = census::run(
        &client,
        Some(&cache),
        &ctx(),
        &census::Args {
            year: 1850,
            place: Some("Leiden".into()),
            gg_uri: None,
            province: None,
            richness: None,
        },
    )
    .unwrap();

    assert_eq!(r.shape, Shape::SingleNested);
    assert_eq!(r.body["census"]["year"], 1850);
}

#[test]
fn census_requires_place_or_gg_uri() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = census::run(
        &client,
        Some(&cache),
        &ctx(),
        &census::Args {
            year: 1850,
            place: None,
            gg_uri: None,
            province: None,
            richness: None,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--place") && err.message().contains("--gg-uri"),
        "message: {}",
        err.message()
    );
}

#[test]
fn census_rejects_both_place_and_gg_uri() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = census::run(
        &client,
        Some(&cache),
        &ctx(),
        &census::Args {
            year: 1850,
            place: Some("Leiden".into()),
            gg_uri: Some("gg:1".into()),
            province: None,
            richness: None,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
}

#[test]
fn census_rejects_richness_outside_1_to_3() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = census::run(
        &client,
        Some(&cache),
        &ctx(),
        &census::Args {
            year: 1850,
            place: Some("Leiden".into()),
            gg_uri: None,
            province: None,
            richness: Some(4),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--richness"));
}

#[test]
fn census_rejects_fields() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.fields = Some(vec!["census".into()]);

    let err = census::run(
        &client,
        Some(&cache),
        &c,
        &census::Args {
            year: 1850,
            place: Some("Leiden".into()),
            gg_uri: None,
            province: None,
            richness: None,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--fields"));
}
