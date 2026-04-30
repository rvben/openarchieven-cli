//! End-to-end dispatch coverage for `main.rs`.
//!
//! Drives every top-level subcommand against a wiremock server via
//! `OPENARCHIEVEN_BASE_URL`. Each test asserts both sides of the wire:
//!
//! * The **request** side via `query_param` matchers — wiremock returns 404
//!   unless the query string carries the expected key/value pairs, so a
//!   regression in argument-to-param mapping fails the test.
//! * The **response** side by parsing stdout as JSON and asserting on the
//!   envelope or scalar fields.

use std::time::Duration;

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

struct Env {
    rt: tokio::runtime::Runtime,
    server: MockServer,
    cache: TempDir,
}

impl Env {
    fn new() -> Self {
        let rt = rt();
        let server = rt.block_on(MockServer::start());
        let cache = tempfile::tempdir().unwrap();
        Self { rt, server, cache }
    }

    fn mount_get(&self, p: &'static str, body: Value) {
        self.rt.block_on(async {
            Mock::given(method("GET"))
                .and(path(p))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&self.server)
                .await;
        });
    }

    /// Mount a 200-with-`body` mock that matches only when the request carries
    /// every `(key, value)` in `params`. Other paths/queries miss this mock.
    fn mount_get_with_params(&self, p: &'static str, params: &[(&'static str, &str)], body: Value) {
        self.rt.block_on(async {
            let mut mock = Mock::given(method("GET")).and(path(p));
            for (k, v) in params {
                mock = mock.and(query_param(*k, *v));
            }
            mock.respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&self.server)
                .await;
        });
    }

    fn mount_status(&self, p: &'static str, status: u16) {
        self.rt.block_on(async {
            Mock::given(method("GET"))
                .and(path(p))
                .respond_with(ResponseTemplate::new(status))
                .mount(&self.server)
                .await;
        });
    }

    fn mount_status_after(&self, p: &'static str, status: u16, after: Duration) {
        self.rt.block_on(async {
            Mock::given(method("GET"))
                .and(path(p))
                .respond_with(ResponseTemplate::new(status).set_delay(after))
                .mount(&self.server)
                .await;
        });
    }

    fn received_request_count(&self) -> usize {
        self.rt
            .block_on(self.server.received_requests())
            .map(|v| v.len())
            .unwrap_or(0)
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("openarchieven").unwrap();
        c.env("OPENARCHIEVEN_BASE_URL", self.server.uri())
            .env("OPENARCHIEVEN_CACHE_DIR", self.cache.path())
            .env("OPENARCHIEVEN_RATE_LIMIT", "1000")
            // Avoid bleed from the user's actual env.
            .env_remove("NO_COLOR")
            .env_remove("OPENARCHIEVEN_OUTPUT")
            .env_remove("OPENARCHIEVEN_CACHE_DISABLE");
        c
    }
}

fn last_json_line(stderr: &[u8]) -> Value {
    let s = String::from_utf8_lossy(stderr);
    let last = s.lines().last().expect("stderr non-empty");
    serde_json::from_str(last).expect("last stderr line is JSON")
}

fn parse_stdout_json(out: &[u8]) -> Value {
    let s = String::from_utf8_lossy(out);
    serde_json::from_str(s.trim()).expect("stdout is JSON")
}

// ---------------------------------------------------------------------------
// Each top-level subcommand below verifies the request-side parameter
// mapping (via query_param matchers) and the response-side rendering.
// ---------------------------------------------------------------------------

#[test]
fn search_dispatch_sends_name_and_renders_json() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/search.json",
        &[("name", "jansen")],
        json!({
            "response": {"numFound": 1, "docs": [{"id": "r-1"}]}
        }),
    );
    let out = env.cmd().args(["search", "jansen"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["id"], "r-1");
    assert_eq!(v["total"], 1);
    assert_eq!(v["paginated"], true);
}

#[test]
fn search_with_fields_filters_keys() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/search.json",
        &[("name", "jansen")],
        json!({
            "response": {"numFound": 1, "docs": [{"id": "r-1", "name": "Jan"}]}
        }),
    );
    let out = env
        .cmd()
        .args(["search", "--fields", "id", "jansen"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    let item = v["items"][0].as_object().unwrap();
    assert_eq!(item.len(), 1);
    assert_eq!(item["id"], "r-1");
    assert!(!item.contains_key("name"));
}

#[test]
fn search_with_unknown_fields_is_validation() {
    let env = Env::new();
    env.mount_get(
        "/records/search.json",
        json!({
            "response": {"numFound": 1, "docs": [{"id": "r-1"}]}
        }),
    );
    let out = env
        .cmd()
        .args(["search", "--fields", "totally_made_up", "jansen"])
        .assert()
        .failure();
    let v = last_json_line(&out.get_output().stderr);
    assert_eq!(v["error"]["kind"], "validation");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("totally_made_up")
    );
}

#[test]
fn show_dispatch_sends_archive_and_identifier() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/show.json",
        &[
            ("archive_code", "elo"),
            ("identifier", "abc"),
            ("lang", "nl"),
        ],
        json!({"record": {"id": "abc", "person": {"name": "Jan"}}}),
    );
    let out = env.cmd().args(["show", "elo", "abc"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["record"]["id"], "abc");
    assert_eq!(v["record"]["person"]["name"], "Jan");
}

#[test]
fn show_404_propagates_not_found_to_stderr() {
    let env = Env::new();
    env.mount_status("/records/show.json", 404);
    let out = env
        .cmd()
        .args(["show", "elo", "missing"])
        .assert()
        .failure();
    let v = last_json_line(&out.get_output().stderr);
    assert_eq!(v["error"]["kind"], "not_found");
    assert_eq!(out.get_output().status.code(), Some(1));
}

#[test]
fn show_upstream_error_envelope_exits_nonzero_with_stderr_error() {
    // When the upstream returns HTTP 200 with {error_code, error_description},
    // the CLI must exit non-zero and emit a JSON error on stderr (not stdout).
    let env = Env::new();
    env.rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/show.json"))
            .and(query_param("archive_code", "ZZZ"))
            .and(query_param("identifier", "12345"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "error_code": 1,
                "error_description": "Invalid archive",
                "request": "show"
            })))
            .mount(&env.server)
            .await;
    });
    let out = env
        .cmd()
        .args(["-o", "json", "show", "ZZZ", "12345"])
        .assert()
        .failure();
    let output = out.get_output();
    assert!(output.stdout.is_empty(), "stdout must be empty on error");
    assert_eq!(output.status.code(), Some(1));
    let v = last_json_line(&output.stderr);
    assert_eq!(v["error"]["kind"], "not_found");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid archive"),
        "message: {}",
        v["error"]["message"]
    );
}

#[test]
fn match_dispatch_sends_name_and_birth_year() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/match.json",
        &[("name", "jansen"), ("birth_year", "1900"), ("lang", "nl")],
        json!({
            "response": {"numFound": 1, "docs": [{"id": "m-1", "score": 0.9}]}
        }),
    );
    let out = env
        .cmd()
        .args(["match", "jansen", "1900"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["id"], "m-1");
    assert_eq!(v["items"][0]["score"], 0.9);
}

#[test]
fn births_dispatch_sends_name_and_pagination() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/getBirths.json",
        &[("name", "jansen"), ("number_show", "10"), ("start", "0")],
        json!({"response": {"numFound": 1, "docs": [{"id": "b-1", "name": "Jan"}]}}),
    );
    let out = env.cmd().args(["births", "jansen"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["id"], "b-1");
    assert_eq!(v["total"], 1);
}

#[test]
fn deaths_dispatch_sends_name() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/getDeaths.json",
        &[("name", "jansen")],
        json!({"response": {"numFound": 1, "docs": [{"id": "d-1"}]}}),
    );
    let out = env.cmd().args(["deaths", "jansen"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["id"], "d-1");
}

#[test]
fn marriages_dispatch_sends_both_partner_names() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/getMarriages.json",
        &[("name", "jansen"), ("name2", "pieters")],
        json!({"response": {"numFound": 1, "docs": [{"id": "m-1", "groom": "Jan"}]}}),
    );
    let out = env
        .cmd()
        .args(["marriages", "jansen", "pieters"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["id"], "m-1");
}

#[test]
fn yearsago_dispatch_sends_years_param() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/yearsago.json",
        &[("yearsago", "100"), ("number_show", "10")],
        json!({"response": {"numFound": 1, "docs": [{"id": "y-1"}]}}),
    );
    let out = env.cmd().args(["yearsago", "100"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["id"], "y-1");
}

#[test]
fn archives_dispatch_renders_list() {
    let env = Env::new();
    env.mount_get(
        "/stats/archives.json",
        json!({"archives": [{"archive_code": "elo", "name": "Eindhoven"}]}),
    );
    let out = env.cmd().arg("archives").assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["archive_code"], "elo");
    assert_eq!(v["items"][0]["name"], "Eindhoven");
    assert_eq!(v["items"].as_array().unwrap().len(), 1);
}

#[test]
fn census_dispatch_sends_place_and_year() {
    // Census responses are passed through verbatim as `single-nested` — the API
    // body is the rendered output.
    let env = Env::new();
    env.mount_get_with_params(
        "/related/census.json",
        &[("year", "1900"), ("place", "amsterdam")],
        json!({
            "year": 1900,
            "place": "Amsterdam",
            "response": {"numFound": 1, "docs": [{"id": "c-1"}]},
        }),
    );
    let out = env
        .cmd()
        .args(["census", "--place", "amsterdam", "--year", "1900"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["year"], 1900);
    assert_eq!(v["response"]["docs"][0]["id"], "c-1");
}

#[test]
fn weather_dispatch_sends_date_and_coordinates() {
    let env = Env::new();
    env.mount_get_with_params(
        "/related/weather.json",
        &[
            ("date", "1900-01-01"),
            ("latitude", "52.0"),
            ("longitude", "4.0"),
            ("lang", "nl"),
        ],
        json!([
            {
                "STN":      {"label": "stationnummer", "value": "344"},
                "YYYYMMDD": {"label": "datum", "value": "19000101"},
                "TG":       {"label": "etmaalgemiddelde temperatuur", "value": "30"},
                "FHX":      {"label": "hoogste uurgemiddelde windsnelheid", "value": "211"}
            }
        ]),
    );
    let out = env
        .cmd()
        .args([
            "weather",
            "--date",
            "1900-01-01",
            "--latitude",
            "52.0",
            "--longitude",
            "4.0",
        ])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["total"], 1);
    assert_eq!(v["paginated"], false);
    assert_eq!(v["items"][0]["YYYYMMDD"]["value"], "19000101");
    assert_eq!(v["items"][0]["STN"]["value"], "344");
    assert_eq!(v["items"][0]["TG"]["value"], "30");
}

#[test]
fn stats_records_dispatch_renders_archive_counts() {
    let env = Env::new();
    env.mount_get(
        "/stats/records.json",
        json!({"records": [{"archive_code": "elo", "count": 100}]}),
    );
    let out = env.cmd().args(["stats", "records"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["archive_code"], "elo");
    assert_eq!(v["items"][0]["count"], 100);
}

#[test]
fn stats_sources_dispatch_renders_archive_counts() {
    let env = Env::new();
    env.mount_get(
        "/stats/sources.json",
        json!({"sources": [{"archive_code": "elo", "count": 5}]}),
    );
    let out = env.cmd().args(["stats", "sources"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["count"], 5);
}

#[test]
fn stats_events_dispatch_renders_archive_counts() {
    let env = Env::new();
    env.mount_get(
        "/stats/events.json",
        json!({"events": [{"archive_code": "elo", "count": 10}]}),
    );
    let out = env.cmd().args(["stats", "events"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["count"], 10);
}

#[test]
fn stats_comments_dispatch_renders_archive_counts() {
    let env = Env::new();
    env.mount_get(
        "/stats/comments.json",
        json!({"comments": [{"archive_code": "elo", "count": 1}]}),
    );
    let out = env.cmd().args(["stats", "comments"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["count"], 1);
}

#[test]
fn stats_familynames_dispatch_sends_place_param() {
    let env = Env::new();
    env.mount_get_with_params(
        "/stats/familynames.json",
        &[("place", "Leiden")],
        json!({"familynames": [{"familyname": "Jansen", "count": 1234}]}),
    );
    let out = env
        .cmd()
        .args(["stats", "familynames", "--place", "Leiden"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["familyname"], "Jansen");
    assert_eq!(v["items"][0]["count"], 1234);
}

#[test]
fn stats_firstnames_dispatch_sends_place_and_year() {
    let env = Env::new();
    env.mount_get_with_params(
        "/stats/firstnames.json",
        &[
            ("eventplace", "Leiden"),
            ("eventyear", "1850"),
            ("number_show", "20"),
        ],
        json!({"response": {"firstnames": [{"firstname": "Jan", "count": 1000}]}}),
    );
    let out = env
        .cmd()
        .args(["stats", "firstnames", "--place", "Leiden", "--year", "1850"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["firstname"], "Jan");
    assert_eq!(v["items"][0]["count"], 1000);
}

#[test]
fn stats_professions_dispatch_sends_place_param() {
    let env = Env::new();
    env.mount_get_with_params(
        "/stats/professions.json",
        &[("place", "Leiden")],
        json!({"professions": [{"profession": "boer", "count": 500}]}),
    );
    let out = env
        .cmd()
        .args(["stats", "professions", "--place", "Leiden"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["items"][0]["profession"], "boer");
}

// ---------------------------------------------------------------------------
// Cache management subcommand dispatch.
// ---------------------------------------------------------------------------

#[test]
fn cache_info_dispatch_returns_zero_entries() {
    let env = Env::new();
    let out = env.cmd().args(["cache", "info"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["entries"], 0);
    assert_eq!(v["bytes"], 0);
}

#[test]
fn cache_clear_with_yes_dispatch_succeeds() {
    let env = Env::new();
    let out = env
        .cmd()
        .args(["cache", "clear", "--yes"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["deleted"], 0);
}

#[test]
fn cache_prune_dispatch_returns_zero_when_empty() {
    let env = Env::new();
    let out = env.cmd().args(["cache", "prune"]).assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["deleted"], 0);
}

// ---------------------------------------------------------------------------
// Error handling paths in main.rs (emit_error + non-validation exit codes).
// ---------------------------------------------------------------------------

#[test]
fn upstream_500_emits_server_error_to_stderr() {
    let env = Env::new();
    env.mount_status("/records/search.json", 500);
    let out = env
        .cmd()
        .args(["search", "--no-cache", "x"])
        .assert()
        .failure();
    let v = last_json_line(&out.get_output().stderr);
    assert_eq!(v["error"]["kind"], "server");
    assert_eq!(out.get_output().status.code(), Some(1));
}

#[test]
fn upstream_400_emits_validation_error_to_stderr() {
    let env = Env::new();
    env.rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/records/search.json"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error_code": "INVALID_PARAM",
                "error_description": "name is required"
            })))
            .mount(&env.server)
            .await;
    });
    let out = env
        .cmd()
        .args(["search", "--no-cache", "x"])
        .assert()
        .failure();
    let v = last_json_line(&out.get_output().stderr);
    assert_eq!(v["error"]["kind"], "validation");
    assert_eq!(v["error"]["upstream_code"], "INVALID_PARAM");
    assert_eq!(v["error"]["upstream_message"], "name is required");
}

#[test]
fn timeout_emits_timeout_error_to_stderr() {
    let env = Env::new();
    env.mount_status_after("/records/search.json", 200, Duration::from_secs(5));
    let out = env
        .cmd()
        .args(["search", "--no-cache", "--timeout", "300ms", "x"])
        .assert()
        .failure();
    let v = last_json_line(&out.get_output().stderr);
    assert_eq!(v["error"]["kind"], "timeout");
}

#[test]
fn error_envelope_has_stable_shape() {
    // The JSON line on stderr must always be `{"error": {...}}` with at minimum
    // `kind` and `message` fields. This guards the agent-facing contract.
    let env = Env::new();
    env.mount_status("/records/show.json", 404);
    let out = env
        .cmd()
        .args(["show", "elo", "missing"])
        .assert()
        .failure();
    let stderr = out.get_output().stderr.clone();
    let last_line = String::from_utf8_lossy(&stderr)
        .lines()
        .last()
        .unwrap()
        .to_string();
    let v: Value = serde_json::from_str(&last_line).unwrap();
    let err = v["error"].as_object().expect("error is an object");
    assert!(err.contains_key("kind"), "missing 'kind' in {err:?}");
    assert!(err.contains_key("message"), "missing 'message' in {err:?}");
    assert_eq!(err["kind"], "not_found");
    // No ANSI escape sequences in the JSON line itself.
    assert!(
        !last_line.contains("\x1b["),
        "JSON line contains ANSI escape: {last_line:?}"
    );
}

#[test]
fn version_subcommand_emits_version_object() {
    let env = Env::new();
    let out = env.cmd().arg("version").assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn schema_subcommand_emits_object_with_commands() {
    let env = Env::new();
    let out = env.cmd().arg("schema").assert().success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert!(v.is_object(), "schema root is an object");
    assert!(
        v["commands"].is_array(),
        "schema has a 'commands' array, got: {}",
        serde_json::to_string(&v).unwrap_or_default()
    );
}

// ---------------------------------------------------------------------------
// Global flag plumbing (--no-cache, --refresh, --output, --cache-ttl, env vars).
// ---------------------------------------------------------------------------

#[test]
fn no_cache_flag_does_not_create_cache_entries() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/search.json",
        &[("name", "jansen")],
        json!({"response": {"numFound": 0, "docs": []}}),
    );
    env.cmd()
        .args(["search", "--no-cache", "jansen"])
        .assert()
        .success();
    // With --no-cache, the cache directory must be empty of entry files.
    let entries: Vec<_> = std::fs::read_dir(env.cache.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            s.ends_with(".json") && s.len() == 64 + 5 // 64 hex + ".json"
        })
        .collect();
    assert!(
        entries.is_empty(),
        "--no-cache must skip writes; found: {entries:?}"
    );
}

#[test]
fn second_call_is_served_from_cache() {
    // Default behavior (no --no-cache): the second invocation hits the cache
    // and never touches the network.
    let env = Env::new();
    env.mount_get_with_params(
        "/records/search.json",
        &[("name", "jansen")],
        json!({"response": {"numFound": 1, "docs": [{"id": "r-1"}]}}),
    );
    env.cmd().args(["search", "jansen"]).assert().success();
    let after_first = env.received_request_count();
    env.cmd().args(["search", "jansen"]).assert().success();
    let after_second = env.received_request_count();
    assert_eq!(
        after_second, after_first,
        "second call should be served from cache; got {after_first} → {after_second}"
    );
}

#[test]
fn refresh_flag_bypasses_cache_read() {
    // Pre-populate the cache by running once, then `--refresh` should re-fetch.
    let env = Env::new();
    env.mount_get_with_params(
        "/records/search.json",
        &[("name", "jansen")],
        json!({"response": {"numFound": 1, "docs": [{"id": "r-1"}]}}),
    );
    env.cmd().args(["search", "jansen"]).assert().success();
    let after_first = env.received_request_count();
    env.cmd()
        .args(["search", "--refresh", "jansen"])
        .assert()
        .success();
    let after_refresh = env.received_request_count();
    assert_eq!(
        after_refresh,
        after_first + 1,
        "--refresh must hit the network again",
    );
}

#[test]
fn output_table_flag_renders_box_drawing_table() {
    let env = Env::new();
    env.mount_get(
        "/records/search.json",
        json!({"response": {"numFound": 1, "docs": [{"id": "r-1"}]}}),
    );
    let out = env
        .cmd()
        .args(["--output", "table", "search", "x"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(stdout.contains("r-1"));
    // The table preset is UTF8_FULL — a real regression to ASCII would lose
    // these box-drawing characters.
    assert!(
        stdout.contains('│') && stdout.contains('─'),
        "expected box-drawing chars in:\n{stdout}",
    );
}

#[test]
fn output_markdown_flag_emits_pipe_table() {
    let env = Env::new();
    env.mount_get(
        "/records/search.json",
        json!({"response": {"numFound": 1, "docs": [{"id": "r-1"}]}}),
    );
    let out = env
        .cmd()
        .args(["--output", "markdown", "search", "x"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "| id |");
    assert_eq!(lines[1], "| --- |");
    assert_eq!(lines[2], "| r-1 |");
}

#[test]
fn cache_disable_env_skips_cache_layer() {
    let env = Env::new();
    env.mount_get_with_params(
        "/records/search.json",
        &[("name", "jansen")],
        json!({"response": {"numFound": 0, "docs": []}}),
    );
    // First call with cache disabled.
    env.cmd()
        .env("OPENARCHIEVEN_CACHE_DISABLE", "1")
        .args(["search", "jansen"])
        .assert()
        .success();
    let after_first = env.received_request_count();
    // Second call also goes to network because cache is disabled.
    env.cmd()
        .env("OPENARCHIEVEN_CACHE_DISABLE", "1")
        .args(["search", "jansen"])
        .assert()
        .success();
    let after_second = env.received_request_count();
    assert_eq!(
        after_second,
        after_first + 1,
        "OPENARCHIEVEN_CACHE_DISABLE must bypass the cache",
    );
}

#[test]
fn cache_ttl_inf_persists_entry_with_no_expiry() {
    // `--cache-ttl inf` writes the cache entry with `expires_at: null`.
    let env = Env::new();
    env.mount_get_with_params(
        "/records/search.json",
        &[("name", "jansen")],
        json!({"response": {"numFound": 0, "docs": []}}),
    );
    env.cmd()
        .args(["search", "--cache-ttl", "inf", "jansen"])
        .assert()
        .success();

    let entries: Vec<_> = std::fs::read_dir(env.cache.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            s.ends_with(".json") && s.len() == 64 + 5
        })
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one cache entry");

    let body = std::fs::read_to_string(entries[0].path()).unwrap();
    let v: Value = serde_json::from_str(&body).unwrap();
    assert!(
        v["expires_at"].is_null(),
        "--cache-ttl inf must produce expires_at=null, got: {}",
        v["expires_at"],
    );
}

#[test]
fn no_cache_and_refresh_are_mutually_exclusive() {
    let env = Env::new();
    let out = env
        .cmd()
        .args(["search", "--no-cache", "--refresh", "jansen"])
        .assert()
        .failure();
    let v = last_json_line(&out.get_output().stderr);
    assert_eq!(v["error"]["kind"], "validation");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("--no-cache") && msg.contains("--refresh"),
        "validation message should name both flags: {msg:?}",
    );
}

// ---------------------------------------------------------------------------
// NO_COLOR env var compliance (no-color.org spec).
// ---------------------------------------------------------------------------

#[test]
fn no_color_one_disables_color_does_not_error() {
    // NO_COLOR=1 must not crash; version output must remain valid JSON.
    let env = Env::new();
    let out = env
        .cmd()
        .env("NO_COLOR", "1")
        .args(["version"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn no_color_empty_string_does_not_disable() {
    // no-color.org: an empty value must not be treated as set — normal output expected.
    let env = Env::new();
    let out = env
        .cmd()
        .env("NO_COLOR", "")
        .args(["version"])
        .assert()
        .success();
    let v = parse_stdout_json(&out.get_output().stdout);
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
}

// ---------------------------------------------------------------------------
// --help: typed-Args migration asserts real positionals/flags shown.
// ---------------------------------------------------------------------------

#[test]
fn births_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["births", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("<NAME>"),
        "help should show NAME positional, got: {s}"
    );
    assert!(
        s.contains("--event-year"),
        "help should mention --event-year: {s}"
    );
    assert!(
        s.contains("--event-place"),
        "help should mention --event-place: {s}"
    );
    assert!(
        s.contains("--event-province"),
        "help should mention --event-province: {s}"
    );
    assert!(
        s.contains("Examples:"),
        "help should have Examples block: {s}"
    );
    assert!(
        s.contains("Pieter Jansen"),
        "Examples must include Pieter Jansen: {s}"
    );
    // Old generic placeholders must be gone.
    assert!(
        !s.contains("[REST]..."),
        "stale REST placeholder visible: {s}"
    );
    assert!(
        !s.contains("deferred to the command"),
        "stale catch-all doc visible: {s}"
    );
}

#[test]
fn deaths_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["deaths", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("<NAME>"), "help: {s}");
    assert!(s.contains("--event-year"), "help: {s}");
    assert!(s.contains("--event-place"), "help: {s}");
    assert!(
        !s.contains("--event-province"),
        "deaths must not advertise --event-province: {s}"
    );
    assert!(s.contains("Examples:"), "help: {s}");
    assert!(s.contains("Anna de Vries"), "help: {s}");
}

#[test]
fn marriages_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["marriages", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("<NAME1>"), "help: {s}");
    assert!(s.contains("<NAME2>"), "help: {s}");
    assert!(s.contains("--event-year"), "help: {s}");
    assert!(s.contains("--event-place"), "help: {s}");
    assert!(s.contains("Examples:"), "help: {s}");
    assert!(s.contains("Pieter Jansen"), "help: {s}");
    assert!(s.contains("Anna de Vries"), "help: {s}");
}

#[test]
fn search_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["search", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("<NAME>"), "help: {s}");
    assert!(s.contains("--archive"), "help: {s}");
    assert!(s.contains("--sort"), "help: {s}");
    assert!(s.contains("--event-place"), "help: {s}");
    assert!(s.contains("--source-type"), "help: {s}");
    assert!(s.contains("Examples:"), "help: {s}");
    assert!(s.contains("Pieter Jansen"), "help: {s}");
}

#[test]
fn search_rejects_sort_zero_at_argument_parse() {
    let dir = tempfile::tempdir().unwrap();
    let assert = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .args(["search", "jansen", "--sort=0"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(stderr.contains("--sort"), "stderr: {stderr}");
    assert!(
        stderr.contains("must be in -6..=-1 or 1..=6"),
        "expected range error in stderr: {stderr}"
    );
}

#[test]
fn search_rejects_sort_out_of_range() {
    let dir = tempfile::tempdir().unwrap();
    for bad in ["--sort=7", "--sort=-7"] {
        let assert = assert_cmd::Command::cargo_bin("openarchieven")
            .unwrap()
            .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
            .args(["search", "jansen", bad])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.contains("must be in -6..=-1 or 1..=6"),
            "expected range error for {bad}: {stderr}"
        );
    }
}

#[test]
fn census_rejects_richness_out_of_range_at_parse() {
    let dir = tempfile::tempdir().unwrap();
    for bad in ["--richness=0", "--richness=4"] {
        let assert = assert_cmd::Command::cargo_bin("openarchieven")
            .unwrap()
            .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
            .args(["census", "--year", "1900", "--place", "x", bad])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.contains("--richness"),
            "expected --richness in rejection for {bad}: {stderr}"
        );
    }
}

#[test]
fn weather_rejects_invalid_decimal_at_parse() {
    let dir = tempfile::tempdir().unwrap();
    let cases: [&[&str]; 2] = [
        &[
            "weather",
            "--date",
            "1900-01-01",
            "--latitude",
            "foo",
            "--longitude",
            "4.0",
        ],
        &[
            "weather",
            "--date",
            "1900-01-01",
            "--latitude",
            "52.0",
            "--longitude",
            "bar",
        ],
    ];
    for argv in cases {
        let assert = assert_cmd::Command::cargo_bin("openarchieven")
            .unwrap()
            .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
            .args(argv)
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.contains("must be a decimal number"),
            "expected decimal-rejection error for {argv:?}: {stderr}"
        );
    }
}

#[test]
fn match_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["match", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("<NAME>"), "help: {s}");
    assert!(s.contains("<BIRTHYEAR>"), "help: {s}");
    assert!(s.contains("Examples:"), "help: {s}");
    assert!(s.contains("Pieter Jansen"), "help: {s}");
}

#[test]
fn yearsago_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["yearsago", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("<YEARS>"), "help: {s}");
    assert!(s.contains("Examples:"), "help: {s}");
    assert!(s.contains("100 years ago"), "help: {s}");
}

#[test]
fn show_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["show", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("<ARCHIVE>"),
        "help must show ARCHIVE positional: {s}"
    );
    assert!(
        s.contains("<IDENTIFIER>"),
        "help must show IDENTIFIER positional: {s}"
    );
    assert!(
        s.contains("Examples:"),
        "help must have Examples block: {s}"
    );
    assert!(
        s.contains("EC1E458F"),
        "Examples must include example identifier: {s}"
    );
    assert!(
        !s.contains("[REST]..."),
        "stale REST placeholder visible: {s}"
    );
}

#[test]
fn weather_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["weather", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("--date"), "help must show --date: {s}");
    assert!(s.contains("--latitude"), "help must show --latitude: {s}");
    assert!(s.contains("--longitude"), "help must show --longitude: {s}");
    assert!(
        s.contains("Examples:"),
        "help must have Examples block: {s}"
    );
    assert!(
        s.contains("1953-02-01"),
        "Examples must include example date: {s}"
    );
    assert!(
        !s.contains("[REST]..."),
        "stale REST placeholder visible: {s}"
    );
}

#[test]
fn census_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["census", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("--year"), "help must show --year: {s}");
    assert!(s.contains("--place"), "help must show --place: {s}");
    assert!(s.contains("--richness"), "help must show --richness: {s}");
    assert!(
        s.contains("Examples:"),
        "help must have Examples block: {s}"
    );
    assert!(
        s.contains("Amsterdam"),
        "Examples must include Amsterdam: {s}"
    );
    assert!(
        !s.contains("[REST]..."),
        "stale REST placeholder visible: {s}"
    );
}

#[test]
fn archives_help_shows_real_args_and_examples() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("openarchieven")
        .unwrap()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .env_remove("NO_COLOR")
        .args(["archives", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("Examples:"),
        "help should have Examples block: {s}"
    );
    assert!(
        s.contains("openarchieven archives"),
        "help must show archives example: {s}"
    );
    assert!(
        !s.contains("[REST]..."),
        "stale REST placeholder visible: {s}"
    );
}
