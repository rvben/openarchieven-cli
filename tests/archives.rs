use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::archives;
use openarchieven::runtime::ApiContext;
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn archives_returns_list_with_total_and_paginated_false() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/stats/archives.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "archives": [
                    {"archive_code": "elo", "archive_name": "Erfgoed Leiden"},
                    {"archive_code": "saa", "archive_name": "Stadsarchief Amsterdam"}
                ]
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: server.uri(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let ctx = ApiContext {
        timeout: Duration::from_secs(2),
        cache_mode: CacheMode::Default,
        cache_ttl_override: None,
        cache_dir: None,
        fields: None,
        limit: None,
        offset: None,
        lang: "nl".into(),
    };

    let r = archives::run(&client, Some(&cache), &ctx).unwrap();
    let envelope = r.list_envelope(None);
    assert_eq!(envelope["paginated"], false);
    assert_eq!(envelope["total"], 2);
    assert_eq!(envelope["items"].as_array().unwrap().len(), 2);
    assert_eq!(envelope["items"][0]["archive_code"], "elo");
}

#[test]
fn archives_rejects_limit_with_validation() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = Client::new(ClientConfig {
        base_url: server.uri(),
        timeout: Duration::from_secs(1),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        cache_mode: CacheMode::Default,
    })
    .unwrap();

    let mut ctx = ApiContext {
        timeout: Duration::from_secs(1),
        cache_mode: CacheMode::Default,
        cache_ttl_override: None,
        cache_dir: None,
        fields: None,
        limit: Some(10),
        offset: None,
        lang: "nl".into(),
    };
    let err = archives::run(&client, Some(&cache), &ctx).unwrap_err();
    assert_eq!(err.kind(), openarchieven::error::ErrorKind::Validation);
    assert!(err.message().contains("--limit"));

    ctx.limit = None;
    ctx.offset = Some(0);
    let err = archives::run(&client, Some(&cache), &ctx).unwrap_err();
    assert_eq!(err.kind(), openarchieven::error::ErrorKind::Validation);
}
