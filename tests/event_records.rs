use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::event_records::CommonFlags;
use openarchieven::commands::{births, deaths, marriages};
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
        limit: Some(5),
        offset: Some(0),
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
fn births_paginates_and_filters() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/getBirths.json"))
            .and(query_param("name", "jansen"))
            .and(query_param("eventyear", "1900"))
            .and(query_param("number_show", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"numFound": 42, "docs": [{"id": "1"}]}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = births::run(
        &client,
        Some(&cache),
        &ctx(),
        &births::Args {
            name: "jansen".into(),
            flags: CommonFlags {
                event_year: Some(1900),
                event_place: None,
                event_province: None,
            },
        },
    )
    .unwrap();

    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["paginated"], true);
    assert_eq!(envelope["total"], 42);
    assert_eq!(envelope["limit"], 5);
    assert_eq!(envelope["offset"], 0);
    assert_eq!(envelope["items"].as_array().unwrap().len(), 1);
}

#[test]
fn deaths_rejects_event_province() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = deaths::run(
        &client,
        Some(&cache),
        &ctx(),
        &deaths::Args {
            name: "jansen".into(),
            flags: CommonFlags {
                event_year: None,
                event_place: None,
                event_province: Some("ZH".into()),
            },
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--event-province"),
        "message: {}",
        err.message()
    );
}

#[test]
fn marriages_sends_both_names() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/getMarriages.json"))
            .and(query_param("name", "Jan"))
            .and(query_param("name2", "Anna"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"numFound": 1, "docs": [{"id": "m1"}]}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = marriages::run(
        &client,
        Some(&cache),
        &ctx(),
        &marriages::Args {
            name1: "Jan".into(),
            name2: "Anna".into(),
            flags: CommonFlags::default(),
        },
    )
    .unwrap();

    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["total"], 1);
    assert_eq!(envelope["paginated"], true);
}

#[test]
fn births_rejects_limit_over_100() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut ctx = ctx();
    ctx.limit = Some(101);

    let err = births::run(
        &client,
        Some(&cache),
        &ctx,
        &births::Args {
            name: "jansen".into(),
            flags: CommonFlags::default(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--limit"),
        "message: {}",
        err.message()
    );
}

#[test]
fn marriages_rejects_event_province() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = marriages::run(
        &client,
        Some(&cache),
        &ctx(),
        &marriages::Args {
            name1: "Jan".into(),
            name2: "Anna".into(),
            flags: CommonFlags {
                event_year: None,
                event_place: None,
                event_province: Some("ZH".into()),
            },
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--event-province"),
        "message: {}",
        err.message()
    );
}

#[test]
fn births_event_place_filter_sent() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/getBirths.json"))
            .and(query_param("name", "jansen"))
            .and(query_param("eventplace", "Amsterdam"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"numFound": 3, "docs": []}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = births::run(
        &client,
        Some(&cache),
        &ctx(),
        &births::Args {
            name: "jansen".into(),
            flags: CommonFlags {
                event_year: None,
                event_place: Some("Amsterdam".into()),
                event_province: None,
            },
        },
    )
    .unwrap();

    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["total"], 3);
}

#[test]
fn births_with_event_province_sent() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/getBirths.json"))
            .and(query_param("name", "jansen"))
            .and(query_param("eventprovince", "ZH"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"numFound": 2, "docs": []}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = births::run(
        &client,
        Some(&cache),
        &ctx(),
        &births::Args {
            name: "jansen".into(),
            flags: CommonFlags {
                event_year: None,
                event_place: None,
                event_province: Some("ZH".into()),
            },
        },
    )
    .unwrap();

    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["total"], 2);
}

#[test]
fn births_rejects_zero_limit() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut ctx = ctx();
    ctx.limit = Some(0);

    let err = births::run(
        &client,
        Some(&cache),
        &ctx,
        &births::Args {
            name: "jansen".into(),
            flags: CommonFlags::default(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"), "msg: {}", err.message());
}

#[test]
fn deaths_rejects_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut ctx = ctx();
    ctx.lang = "en".into();

    let err = deaths::run(
        &client,
        Some(&cache),
        &ctx,
        &deaths::Args {
            name: "jansen".into(),
            flags: CommonFlags::default(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--lang"), "msg: {}", err.message());
}

#[test]
fn marriages_with_event_year_filter() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/getMarriages.json"))
            .and(query_param("name", "Jan"))
            .and(query_param("name2", "Anna"))
            .and(query_param("eventyear", "1900"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"numFound": 1, "docs": [{"id": "m1"}]}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = marriages::run(
        &client,
        Some(&cache),
        &ctx(),
        &marriages::Args {
            name1: "Jan".into(),
            name2: "Anna".into(),
            flags: CommonFlags {
                event_year: Some(1900),
                event_place: None,
                event_province: None,
            },
        },
    )
    .unwrap();

    let envelope = r.list_envelope(r.total);
    assert_eq!(envelope["total"], 1);
}

#[test]
fn births_filters_year_clientside_when_upstream_returns_mixed() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/getBirths.json"))
            .and(query_param("name", "jansen"))
            .and(query_param("eventyear", "1900"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"numFound": 3, "docs": [
                    {"id": "a", "eventdate": {"year": 1900}},
                    {"id": "b", "eventdate": {"year": 1788}},
                    {"id": "c", "eventdate": {"year": 1900}},
                ]}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = births::run(
        &client,
        Some(&cache),
        &ctx(),
        &births::Args {
            name: "jansen".into(),
            flags: CommonFlags {
                event_year: Some(1900),
                event_place: None,
                event_province: None,
            },
        },
    )
    .unwrap();

    let envelope = r.list_envelope(r.total);
    let items = envelope["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        2,
        "expected 2 docs after client-side year filter"
    );
    assert!(
        items.iter().all(|d| d["eventdate"]["year"] == 1900),
        "all returned docs must have eventdate.year == 1900"
    );
}

#[test]
fn births_keeps_docs_without_eventdate_year_when_event_year_set() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/getBirths.json"))
            .and(query_param("name", "jansen"))
            .and(query_param("eventyear", "1900"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {"numFound": 3, "docs": [
                    {"id": "a", "eventdate": {"year": 1900}},
                    {"id": "b"},
                    {"id": "c", "eventdate": {"year": 1850}},
                ]}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = births::run(
        &client,
        Some(&cache),
        &ctx(),
        &births::Args {
            name: "jansen".into(),
            flags: CommonFlags {
                event_year: Some(1900),
                event_place: None,
                event_province: None,
            },
        },
    )
    .unwrap();

    let envelope = r.list_envelope(r.total);
    let items = envelope["items"].as_array().unwrap();
    // doc "a" (year 1900) and doc "b" (no eventdate.year) are kept; doc "c" (year 1850) is dropped
    assert_eq!(items.len(), 2, "docs missing eventdate.year must be kept");
    let ids: Vec<_> = items.iter().map(|d| d["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"a"), "doc with matching year must be kept");
    assert!(
        ids.contains(&"b"),
        "doc without eventdate.year must be kept"
    );
    assert!(
        !ids.contains(&"c"),
        "doc with non-matching year must be dropped"
    );
}
