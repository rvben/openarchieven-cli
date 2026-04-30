use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::transcripts::search;
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
fn search_passes_required_query_and_pagination() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/search.json"))
            .and(query_param("q", "coret"))
            .and(query_param("number_show", "5"))
            .and(query_param("start", "10"))
            .and(query_param("lang", "en"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "query": {
                    "q": "coret",
                    "archive_code": null,
                    "archive_number": null,
                    "inventory_number": null,
                    "year_start": null,
                    "year_end": null,
                    "start": 10,
                    "number_show": 5,
                    "language": "en"
                },
                "response": {
                    "number_found": 1128,
                    "docs": [
                        {"id": "NL-HaNA_1.04.02_8068_0088", "page": "88"},
                        {"id": "NL-HaNA_1.04.02_8068_0090", "page": "90"}
                    ],
                    "facets": {
                        "source_archive": [
                            {"isil": "NL-HaNA", "archive_code": "rzh", "name": "Nationaal Archief", "count": 986}
                        ]
                    }
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
        q: "coret".into(),
        ..Default::default()
    };

    let r = search::run(&client, Some(&cache), &ctx, &args).unwrap();
    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["paginated"], true);
    assert_eq!(envelope["total"], 1128);
    assert_eq!(envelope["limit"], 5);
    assert_eq!(envelope["offset"], 10);
    assert_eq!(envelope["items"].as_array().unwrap().len(), 2);
    assert_eq!(envelope["items"][0]["id"], "NL-HaNA_1.04.02_8068_0088");
}

#[test]
fn search_sends_optional_filters() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/search.json"))
            .and(query_param("q", "coret"))
            .and(query_param("archive_code", "hua"))
            .and(query_param("archive_number", "67"))
            .and(query_param("inventory_number", "53"))
            .and(query_param("year_start", "1700"))
            .and(query_param("year_end", "1750"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"number_found": 1, "docs": [{"id": "x"}]}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let args = search::Args {
        q: "coret".into(),
        archive_code: Some("hua".into()),
        archive_number: Some("67".into()),
        inventory_number: Some("53".into()),
        year_start: Some(1700),
        year_end: Some(1750),
    };

    let r = search::run(&client, Some(&cache), &ctx(), &args).unwrap();
    assert_eq!(r.total, Some(1));
}

#[test]
fn search_rejects_unsupported_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.lang = "es".into();

    let args = search::Args {
        q: "coret".into(),
        ..Default::default()
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
fn search_accepts_de_and_fr_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/search.json"))
            .and(query_param("lang", "de"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"number_found": 0, "docs": []}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.lang = "de".into();

    let args = search::Args {
        q: "x".into(),
        ..Default::default()
    };

    let r = search::run(&client, Some(&cache), &ctx, &args).unwrap();
    assert_eq!(r.total, Some(0));
}

#[test]
fn search_caps_limit_at_100() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.limit = Some(150);

    let args = search::Args {
        q: "coret".into(),
        ..Default::default()
    };

    let err = search::run(&client, Some(&cache), &ctx, &args).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("100"), "msg: {}", err.message());
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
        q: "coret".into(),
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
fn search_rejects_empty_q() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let args = search::Args {
        q: "".into(),
        ..Default::default()
    };

    let err = search::run(&client, Some(&cache), &ctx(), &args).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("q"), "msg: {}", err.message());
}

#[test]
fn search_handles_empty_response() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/search.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let args = search::Args {
        q: "coret".into(),
        ..Default::default()
    };

    let r = search::run(&client, Some(&cache), &ctx(), &args).unwrap();
    assert!(r.total.is_none());
    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["items"].as_array().unwrap().len(), 0);
}

#[test]
fn search_schema_contract() {
    let s = search::schema();
    assert_eq!(s.name, "transcripts search");
    assert!(s.paginated);
    assert_eq!(s.response_shape, "list");
    let q = s.args.iter().find(|a| a.name == "q").unwrap();
    assert!(q.required);
    assert!(q.positional);
    let lang = s.args.iter().find(|a| a.name == "--lang").unwrap();
    let lang_enum = lang.r#enum.as_ref().unwrap();
    let langs: Vec<&str> = lang_enum.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(langs, vec!["nl", "en", "de", "fr"]);
    let limit = s.args.iter().find(|a| a.name == "--limit").unwrap();
    assert_eq!(limit.max, Some(100));
}
