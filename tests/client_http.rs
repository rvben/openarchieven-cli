use std::time::Duration;

use openarchieven::client::{Client, ClientConfig};
use openarchieven::error::ErrorKind;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn refused_client() -> Client {
    // Port 1 is generally reserved and will produce a connection refused error
    // quickly without any OS-level delay.
    Client::new(ClientConfig {
        base_url: "http://127.0.0.1:1".to_string(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        ..ClientConfig::default()
    })
    .unwrap()
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn start_server() -> (tokio::runtime::Runtime, MockServer) {
    let rt = rt();
    let server = rt.block_on(MockServer::start());
    (rt, server)
}

fn client(base_url: &str) -> Client {
    Client::new(ClientConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(2),
        lang: "nl".into(),
        rate_limit_per_sec: 1000,
        ..ClientConfig::default()
    })
    .unwrap()
}

#[test]
fn execute_once_200_returns_body() {
    let (rt, server) = start_server();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"records": []})))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri());
    let v = c.execute_once("/records/search", &[("name", "x")]).unwrap();
    assert_eq!(v["records"].as_array().unwrap().len(), 0);
}

#[test]
fn execute_once_400_with_structured_body_is_validation() {
    let (rt, server) = start_server();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error_code": "INVALID_PARAM",
                "error_description": "name is required"
            })))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri());
    let err = c.execute_once("/records/search", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert_eq!(err.upstream_code(), Some("INVALID_PARAM"));
    assert_eq!(err.upstream_message(), Some("name is required"));
}

#[test]
fn execute_once_404_is_not_found() {
    let (rt, server) = start_server();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/show"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri());
    let err = c.execute_once("/records/show", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[test]
fn execute_once_429_is_rate_limit_with_retry_after() {
    let (rt, server) = start_server();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "30"))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri());
    let err = c.execute_once("/records/search", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimit);
    assert_eq!(err.retry_after_seconds(), Some(30));
}

#[test]
fn execute_once_500_is_server() {
    let (rt, server) = start_server();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri());
    let err = c.execute_once("/records/search", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Server);
}

#[test]
fn execute_once_2xx_unparseable_is_parse() {
    let (rt, server) = start_server();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>not json</html>"))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri());
    let err = c.execute_once("/records/search", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Parse);
}

#[test]
fn execute_once_connection_refused_is_network_error() {
    let c = refused_client();
    let err = c.execute_once("/any", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Network);
}

#[test]
fn execute_once_400_with_non_json_body_is_validation() {
    let (rt, server) = start_server();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search"))
            .respond_with(ResponseTemplate::new(400).set_body_string("plain error text"))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri());
    let err = c.execute_once("/records/search", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.upstream_code().is_none());
}

#[test]
fn execute_once_400_with_json_but_no_error_code_is_validation() {
    let (rt, server) = start_server();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"message": "bad input"})))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri());
    let err = c.execute_once("/records/search", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.upstream_code().is_none());
}

#[test]
fn execute_once_unexpected_status_is_server() {
    let (rt, server) = start_server();
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search"))
            .respond_with(ResponseTemplate::new(418))
            .mount(&server)
            .await;
    });
    let c = client(&server.uri());
    let err = c.execute_once("/records/search", &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Server);
}
