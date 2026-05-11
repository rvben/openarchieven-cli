use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::transcripts::show;
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
fn show_returns_nested_transcription() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/show.json"))
            .and(query_param("id", "NL-SdmGA_1504889_11"))
            .and(query_param("lang", "nl"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "NL-SdmGA_1504889_11",
                "uri": "https://www.openarchieven.nl/transcripties/toon/NL-SdmGA_1504889_11",
                "source_archive": {"isil": "NL-SdmGA", "archive_code": "sch", "name": "Gemeentearchief Schiedam"},
                "page": "11",
                "project": "transkribus",
                "transcript": "Bijden Innehoude vanden jegenwoordigen Instrumente..."
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
            id: "NL-SdmGA_1504889_11".into(),
        },
    )
    .unwrap();

    assert_eq!(r.shape, openarchieven::output::Shape::SingleNested);
    assert_eq!(r.body["id"], "NL-SdmGA_1504889_11");
    assert_eq!(r.body["project"], "transkribus");
    assert_eq!(
        r.body["transcript"],
        "Bijden Innehoude vanden jegenwoordigen Instrumente..."
    );
}

#[test]
fn show_404_is_not_found() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/show.json"))
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
            id: "missing".into(),
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
            .and(path("/transcriptions/show.json"))
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
        &show::Args { id: "ghost".into() },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert!(
        err.message().contains("ghost"),
        "message was: {}",
        err.message()
    );
}

#[test]
fn show_in_body_error_envelope_is_not_found() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/transcriptions/show.json"))
            .and(query_param("id", "NL-XX_0_0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "error_code": 2,
                "error_description": "Transcription not found"
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
            id: "NL-XX_0_0".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(
        err.message(),
        "no transcription found for NL-XX_0_0 (upstream: Transcription not found)"
    );
}

#[test]
fn show_rejects_empty_id() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let err = show::run(&client, Some(&cache), &ctx(), &show::Args { id: "".into() }).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("id"), "msg: {}", err.message());
}

#[test]
fn show_rejects_unsupported_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.lang = "es".into();

    let err = show::run(
        &client,
        Some(&cache),
        &ctx,
        &show::Args {
            id: "NL-X_1_1".into(),
        },
    )
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--lang"), "msg: {}", err.message());
}

#[test]
fn show_rejects_pagination_flags() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = client(&server);

    let mut ctx = ctx();
    ctx.limit = Some(5);

    let err = show::run(
        &client,
        Some(&cache),
        &ctx,
        &show::Args {
            id: "NL-X_1_1".into(),
        },
    )
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
}

#[test]
fn show_schema_contract() {
    let s = show::schema();
    assert_eq!(s.name, "transcripts show");
    assert_eq!(s.response_shape, "single-nested");
    assert!(!s.paginated);
    assert_eq!(s.cache_ttl_strategy, "never");
    let id = s.args.iter().find(|a| a.name == "id").unwrap();
    assert!(id.required);
    assert!(id.positional);
}
