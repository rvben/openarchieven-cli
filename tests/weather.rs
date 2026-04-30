use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands::weather;
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
fn weather_returns_list() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/related/weather.json"))
            .and(query_param("date", "1850-06-15"))
            .and(query_param("longitude", "4.49"))
            .and(query_param("latitude", "52.16"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "STN": {"label": "stationnummer", "value": "344"},
                    "YYYYMMDD": {"label": "datum", "value": "18500615"},
                    "TG": {"label": "etmaalgemiddelde temperatuur (in 0.1 graden Celsius)", "value": "180"},
                    "FHX": {"label": "hoogste uurgemiddelde windsnelheid (in 0.1 m/s)", "value": "45"}
                }
            ])))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = weather::run(
        &client,
        Some(&cache),
        &ctx(),
        &weather::Args {
            date: "1850-06-15".into(),
            longitude: "4.49".into(),
            latitude: "52.16".into(),
        },
    )
    .unwrap();

    assert_eq!(r.shape, Shape::List);
    let env = r.list_envelope(Some(1));
    assert_eq!(env["total"], 1);
    assert_eq!(env["items"][0]["STN"]["value"], "344");
}

#[test]
fn weather_tolerates_single_object_response() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/related/weather.json"))
            .and(query_param("date", "1850-06-15"))
            .and(query_param("longitude", "4.49"))
            .and(query_param("latitude", "52.16"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "STN": {"label": "stationnummer", "value": "344"},
                "TG": {"label": "temp", "value": "180"}
            })))
            .mount(&server)
            .await;
    });

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let r = weather::run(
        &client,
        Some(&cache),
        &ctx(),
        &weather::Args {
            date: "1850-06-15".into(),
            longitude: "4.49".into(),
            latitude: "52.16".into(),
        },
    )
    .unwrap();

    // Single-object upstream response is wrapped as a one-item list.
    assert_eq!(r.shape, Shape::List);
    let env = r.list_envelope(Some(1));
    assert_eq!(env["total"], 1);
    assert_eq!(env["items"][0]["STN"]["value"], "344");
}

#[test]
fn weather_validates_date_format() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = weather::run(
        &client,
        Some(&cache),
        &ctx(),
        &weather::Args {
            date: "not-a-date".into(),
            longitude: "4.49".into(),
            latitude: "52.16".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--date"));
}

#[test]
fn weather_validates_longitude_decimal() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = weather::run(
        &client,
        Some(&cache),
        &ctx(),
        &weather::Args {
            date: "1850-06-15".into(),
            longitude: "not-a-number".into(),
            latitude: "52.16".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--longitude"));
}

#[test]
fn weather_rejects_unknown_lang() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.lang = "de".into();

    let err = weather::run(
        &client,
        Some(&cache),
        &c,
        &weather::Args {
            date: "1850-06-15".into(),
            longitude: "4.49".into(),
            latitude: "52.16".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--lang"));
}

#[test]
fn weather_rejects_offset() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let mut c = ctx();
    c.offset = Some(5);

    let err = weather::run(
        &client,
        Some(&cache),
        &c,
        &weather::Args {
            date: "1850-06-15".into(),
            longitude: "4.49".into(),
            latitude: "52.16".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--limit"), "msg: {}", err.message());
}

#[test]
fn weather_validates_latitude_decimal() {
    let rt = rt();
    let server = rt.block_on(MockServer::start());

    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let client = make_client(&server);

    let err = weather::run(
        &client,
        Some(&cache),
        &ctx(),
        &weather::Args {
            date: "1850-06-15".into(),
            longitude: "4.49".into(),
            latitude: "not-a-number".into(),
        },
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(
        err.message().contains("--latitude"),
        "msg: {}",
        err.message()
    );
}
