use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::transcripts::browse;
use openarchieven::error::ErrorKind;
use openarchieven::runtime::ApiContext;
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::query_param_is_missing;
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
fn browse_level1_no_filters_lists_archives() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/browse.json"))
            .and(query_param("lang", "nl"))
            .and(query_param_is_missing("archive_code"))
            .and(query_param_is_missing("archive_number"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "filters": {"archive_code": null, "archive_number": null},
                "response": {
                    "level": 1,
                    "docs": [
                        {"isil": "NL-HaNA", "archive_code": "rzh", "name": "Nationaal Archief", "count": 15841597},
                        {"isil": "NL-AsdSAA", "archive_code": "saa", "name": "Stadsarchief Amsterdam", "count": 2966344}
                    ]
                }
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let r = browse::run(&client, Some(&cache), &ctx(), &browse::Args::default()).unwrap();
    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["paginated"], false);
    assert_eq!(envelope["total"], 2);
    assert_eq!(envelope["items"].as_array().unwrap().len(), 2);
    assert_eq!(envelope["items"][0]["archive_code"], "rzh");
}

#[test]
fn browse_level2_with_archive_code_lists_archive_numbers() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/browse.json"))
            .and(query_param("archive_code", "hua"))
            .and(query_param_is_missing("archive_number"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "filters": {"archive_code": "hua", "archive_number": null},
                "response": {
                    "level": 2,
                    "docs": [
                        {"nr": "34-4", "title": "Notarissen Utrecht", "count": 91409, "url": "https://example/x"},
                        {"nr": "67", "title": "Familie Huydecoper", "count": 1420, "url": "https://example/y"}
                    ]
                }
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let args = browse::Args {
        archive_code: Some("hua".into()),
        archive_number: None,
    };
    let r = browse::run(&client, Some(&cache), &ctx(), &args).unwrap();
    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["total"], 2);
    assert_eq!(envelope["items"][0]["nr"], "34-4");
}

#[test]
fn browse_level3_with_archive_code_and_number() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/browse.json"))
            .and(query_param("archive_code", "hua"))
            .and(query_param("archive_number", "67"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "filters": {"archive_code": "hua", "archive_number": "67"},
                "response": {
                    "level": 3,
                    "docs": [
                        {"nr": "53", "title": "Brieven 1648", "count": 81, "url": "https://example/53"}
                    ]
                }
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let args = browse::Args {
        archive_code: Some("hua".into()),
        archive_number: Some("67".into()),
    };
    let r = browse::run(&client, Some(&cache), &ctx(), &args).unwrap();
    assert_eq!(r.total, Some(1));
}

#[test]
fn browse_rejects_archive_number_without_archive_code() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let args = browse::Args {
        archive_code: None,
        archive_number: Some("67".into()),
    };
    let err = browse::run(&client, Some(&cache), &ctx(), &args).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--archive-code"),
        "msg: {}",
        err.message()
    );
}

#[test]
fn browse_rejects_pagination_flags() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.limit = Some(5);

    let err = browse::run(&client, Some(&cache), &ctx, &browse::Args::default()).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
}

#[test]
fn browse_rejects_offset() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.offset = Some(5);

    let err = browse::run(&client, Some(&cache), &ctx, &browse::Args::default()).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
}

#[test]
fn browse_rejects_unsupported_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.lang = "es".into();

    let err = browse::run(&client, Some(&cache), &ctx, &browse::Args::default()).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
}

#[test]
fn browse_schema_contract() {
    let s = browse::schema();
    assert_eq!(s.name, "transcripts browse");
    assert!(!s.paginated);
    assert_eq!(s.response_shape, "list");
    assert!(s.args.iter().any(|a| a.name == "--archive-code"));
    assert!(s.args.iter().any(|a| a.name == "--archive-number"));
    assert_eq!(s.cache_ttl_strategy, "fixed");
    let week = 7 * 24 * 3600u64;
    assert_eq!(s.cache_ttl_seconds, Some(week));
}
