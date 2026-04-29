use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::match_record;
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
fn match_returns_list_non_paginated() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/match.json"))
            .and(query_param("name", "Jan Jansen"))
            .and(query_param("birth_year", "1850"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {
                    "docs": [{"id": "x"}, {"id": "y"}]
                }
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let r = match_record::run(
        &client,
        Some(&cache),
        &ctx(),
        &match_record::Args {
            name: "Jan Jansen".into(),
            birth_year: 1850,
        },
    )
    .unwrap();

    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["paginated"], false);
    assert_eq!(envelope["total"], 2);
    assert_eq!(envelope["items"].as_array().unwrap().len(), 2);
}

#[test]
fn match_rejects_limit() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.limit = Some(10);

    let err = match_record::run(
        &client,
        Some(&cache),
        &ctx,
        &match_record::Args {
            name: "Jan Jansen".into(),
            birth_year: 1850,
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
}
