//! Wire-format contract tests against the vendored OpenAPI manifest.
//!
//! Three layers of assertions guard against silent parameter drift:
//!
//! 1. **Per-endpoint contract** — each wrapped endpoint has a `*_request_matches_spec`
//!    test that fires one request with every CLI flag populated and asserts the
//!    *exact set* of outbound query keys equals what the test declares. Catches:
//!      - typoed wire keys that the upstream silently drops (the original bug),
//!      - forgotten `params.push(...)` (key absent though the flag is set),
//!      - keys in the request that the spec doesn't recognise.
//!
//! 2. **Endpoint coverage** — `every_spec_endpoint_is_wrapped` walks the manifest
//!    and fails if a spec path is neither in `WRAPPED_PATHS` nor explicitly
//!    listed in `INTENTIONALLY_UNWRAPPED`. New upstream endpoints can't slip
//!    through silently — the test forces a deliberate decision.
//!
//! 3. **Drift detection** — `make openapi-check` (and a weekly CI cron) compare
//!    `openapi/openarchieven.sha256` against the live spec at
//!    `https://api.openarchieven.nl/openapi.yaml`. A drift opens a refresh PR.
//!
//! The manifest is regenerated from the live spec by `make openapi-refresh`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::OnceLock;
use std::time::Duration;

use openarchieven::cache::Cache;
use openarchieven::client::{CacheMode, Client, ClientConfig};
use openarchieven::commands;
use openarchieven::commands::event_records::CommonFlags;
use openarchieven::commands::stats::archive_stat::ArchiveStatArgs;
use openarchieven::runtime::ApiContext;
use serde::Deserialize;
use serde_json::json;
use tempfile::{TempDir, tempdir};
use url::Url;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const MANIFEST_JSON: &str = include_str!("../openapi/params-manifest.json");

/// Spec paths that have a wrapping command and a contract test below.
/// Keep in sync with the `*_request_matches_spec` tests.
const WRAPPED_PATHS: &[&str] = &[
    "/records/search.json",
    "/records/show.json",
    "/records/match.json",
    "/records/getBirths.json",
    "/records/getDeaths.json",
    "/records/getMarriages.json",
    "/records/yearsago.json",
    "/stats/archives.json",
    "/stats/breakdown.json",
    "/stats/comments.json",
    "/stats/events.json",
    "/stats/familynames.json",
    "/stats/firstnames.json",
    "/stats/professions.json",
    "/stats/records.json",
    "/stats/sources.json",
    "/related/census.json",
    "/related/weather.json",
    "/transcriptions/browse.json",
    "/transcriptions/search.json",
    "/transcriptions/show.json",
];

/// Spec paths intentionally not surfaced as a CLI command. Empty by design —
/// add a path here only with a comment explaining why. The coverage test fails
/// if the spec contains anything that's not in `WRAPPED_PATHS ∪ this set`.
const INTENTIONALLY_UNWRAPPED: &[&str] = &[];

#[derive(Debug, Deserialize)]
struct Operation {
    #[serde(rename = "operationId")]
    operation_id: String,
    method: String,
    query_params: Vec<String>,
}

fn manifest() -> &'static BTreeMap<String, Operation> {
    static CELL: OnceLock<BTreeMap<String, Operation>> = OnceLock::new();
    CELL.get_or_init(|| {
        serde_json::from_str(MANIFEST_JSON)
            .expect("openapi/params-manifest.json must be valid JSON; run `make openapi-refresh`")
    })
}

struct Harness {
    rt: tokio::runtime::Runtime,
    server: MockServer,
    client: Client,
    cache: Cache,
    _dir: TempDir,
    ctx: ApiContext,
}

impl Harness {
    fn new() -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(MockServer::start());
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
            quiet: true,
        };
        Self {
            rt,
            server,
            client,
            cache,
            _dir: dir,
            ctx,
        }
    }

    /// Mount a generic 200 response that matches any GET. The body is irrelevant
    /// for spec-conformance — we only care about what the CLI sends.
    fn mount(&self, body: serde_json::Value) {
        self.rt.block_on(async {
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&self.server)
                .await;
        });
    }

    /// Assert the exact wire-level contract for `path`:
    ///   1. every outbound query key is allowed by the OpenAPI spec, AND
    ///   2. the set of outbound keys equals `expected` (catches forgotten
    ///      `params.push(...)` even when the CLI flag is set).
    fn assert_spec_contract(&self, path: &str, expected: &[&str]) {
        let op = manifest().get(path).unwrap_or_else(|| {
            panic!(
                "no entry for {path} in openapi/params-manifest.json; run `make openapi-refresh`"
            )
        });
        let allowed: HashSet<&str> = op.query_params.iter().map(String::as_str).collect();
        let expected_set: BTreeSet<&str> = expected.iter().copied().collect();
        // Surface contract drift early: expected ⊆ spec.allowed.
        for k in &expected_set {
            assert!(
                allowed.contains(k),
                "{path} ({}, {}): expected key `{k}` is not in OpenAPI spec.\n  spec allows: {:?}",
                op.method,
                op.operation_id,
                op.query_params
            );
        }

        let reqs = self
            .rt
            .block_on(self.server.received_requests())
            .unwrap_or_default();
        let matched: Vec<_> = reqs.iter().filter(|r| r.url.path() == path).collect();
        assert!(
            !matched.is_empty(),
            "no requests captured for {path}; sent paths: {:?}",
            reqs.iter().map(|r| r.url.path()).collect::<Vec<_>>()
        );

        for req in matched {
            let url = Url::parse(req.url.as_str()).expect("captured URL parses");
            let actual: BTreeSet<String> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
            let actual_refs: BTreeSet<&str> = actual.iter().map(String::as_str).collect();

            // Layer 1: every outbound key is in spec.
            for key in &actual_refs {
                assert!(
                    allowed.contains(*key),
                    "{path} ({}, {}): outbound query param `{key}` is not in OpenAPI spec.\n  spec allows: {:?}\n  full URL: {url}",
                    op.method,
                    op.operation_id,
                    op.query_params
                );
            }

            // Layer 2: outbound set equals expected. Catches `params.push` omissions.
            let missing: Vec<&&str> = expected_set.difference(&actual_refs).collect();
            let unexpected: Vec<&&str> = actual_refs.difference(&expected_set).collect();
            assert!(
                missing.is_empty() && unexpected.is_empty(),
                "{path} ({}, {}): outbound keys disagree with expected.\n  expected:   {:?}\n  actual:     {:?}\n  missing:    {:?}\n  unexpected: {:?}\n  full URL:   {url}",
                op.method,
                op.operation_id,
                expected_set,
                actual_refs,
                missing,
                unexpected
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Coverage: every spec endpoint must be wrapped or explicitly skipped.
// ---------------------------------------------------------------------------

#[test]
fn every_spec_endpoint_is_wrapped() {
    let manifest_paths: BTreeSet<&str> = manifest().keys().map(String::as_str).collect();
    let wrapped: BTreeSet<&str> = WRAPPED_PATHS.iter().copied().collect();
    let skipped: BTreeSet<&str> = INTENTIONALLY_UNWRAPPED.iter().copied().collect();

    let covered: BTreeSet<&str> = wrapped.union(&skipped).copied().collect();
    let uncovered: Vec<&&str> = manifest_paths.difference(&covered).collect();
    assert!(
        uncovered.is_empty(),
        "OpenAPI spec contains endpoints with no CLI wrapping and no entry in \
         INTENTIONALLY_UNWRAPPED:\n  {uncovered:?}\n\n\
         Either wrap them as a new command, or add them to INTENTIONALLY_UNWRAPPED \
         with a comment explaining why."
    );

    let stale: Vec<&&str> = wrapped.difference(&manifest_paths).collect();
    assert!(
        stale.is_empty(),
        "WRAPPED_PATHS lists endpoints that no longer exist in the OpenAPI spec:\n  \
         {stale:?}\n\nDid upstream rename or remove them? Run `make openapi-refresh`."
    );

    let dead_skips: Vec<&&str> = skipped.difference(&manifest_paths).collect();
    assert!(
        dead_skips.is_empty(),
        "INTENTIONALLY_UNWRAPPED lists endpoints no longer in the OpenAPI spec:\n  \
         {dead_skips:?}\n\nClean up the allowlist."
    );
}

// ---------------------------------------------------------------------------
// /records/search.json
// ---------------------------------------------------------------------------

#[test]
fn search_request_matches_spec() {
    let mut h = Harness::new();
    h.mount(json!({"response": {"docs": [], "numFound": 0}}));
    h.ctx.lang = "nl".into();
    h.ctx.limit = Some(5);
    h.ctx.offset = Some(0);

    let args = commands::search::Args {
        name: "jansen".into(),
        archive: Some("elo".into()),
        source_type: Some("BS Geboorte".into()),
        event_place: Some("Rotterdam".into()),
        birth_place: Some("Leiden".into()),
        relation_type: Some("vader".into()),
        country: Some("nl".into()),
        sort: Some(1),
    };
    commands::search::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/records/search.json",
        &[
            "name",
            "number_show",
            "start",
            "lang",
            "archive_code",
            "sourcetype",
            "eventplace",
            "birthplace",
            "relationtype",
            "country_code",
            "sort",
        ],
    );
}

// ---------------------------------------------------------------------------
// /records/show.json
// ---------------------------------------------------------------------------

#[test]
fn show_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!({"identifier": "x", "archive_code": "elo"}));

    let args = commands::show::Args {
        archive: "elo".into(),
        identifier: "abc-123".into(),
    };
    commands::show::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract("/records/show.json", &["archive", "identifier", "lang"]);
}

// ---------------------------------------------------------------------------
// /records/match.json
// ---------------------------------------------------------------------------

#[test]
fn match_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!({"response": {"docs": [], "numFound": 0}}));

    let args = commands::match_record::Args {
        name: "jansen".into(),
        birthyear: 1898,
    };
    commands::match_record::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract("/records/match.json", &["name", "birthyear", "lang"]);
}

// ---------------------------------------------------------------------------
// /records/getBirths.json
// ---------------------------------------------------------------------------

#[test]
fn births_request_matches_spec() {
    let mut h = Harness::new();
    h.mount(json!({"response": {"docs": [], "numFound": 0}}));
    h.ctx.limit = Some(5);
    h.ctx.offset = Some(0);

    let args = commands::births::Args {
        name: "jansen".into(),
        flags: CommonFlags {
            event_year: Some(1898),
            event_place: Some("Rotterdam".into()),
            event_province: Some("ZH".into()),
        },
    };
    commands::births::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/records/getBirths.json",
        &[
            "name",
            "number_show",
            "start",
            "eventyear",
            "eventplace",
            "eventprovince",
        ],
    );
}

// ---------------------------------------------------------------------------
// /records/getDeaths.json
// ---------------------------------------------------------------------------

#[test]
fn deaths_request_matches_spec() {
    let mut h = Harness::new();
    h.mount(json!({"response": {"docs": [], "numFound": 0}}));
    h.ctx.limit = Some(5);
    h.ctx.offset = Some(0);

    let args = commands::deaths::Args {
        name: "jansen".into(),
        flags: CommonFlags {
            event_year: Some(1918),
            event_place: Some("Amsterdam".into()),
            event_province: None,
        },
    };
    commands::deaths::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/records/getDeaths.json",
        &["name", "number_show", "start", "eventyear", "eventplace"],
    );
}

// ---------------------------------------------------------------------------
// /records/getMarriages.json
// ---------------------------------------------------------------------------

#[test]
fn marriages_request_matches_spec() {
    let mut h = Harness::new();
    h.mount(json!({"response": {"docs": [], "numFound": 0}}));
    h.ctx.limit = Some(5);
    h.ctx.offset = Some(0);

    let args = commands::marriages::Args {
        name1: "jansen".into(),
        name2: "de vries".into(),
        flags: CommonFlags {
            event_year: Some(1925),
            event_place: Some("Utrecht".into()),
            event_province: None,
        },
    };
    commands::marriages::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/records/getMarriages.json",
        &[
            "name1",
            "name2",
            "number_show",
            "start",
            "eventyear",
            "eventplace",
        ],
    );
}

// ---------------------------------------------------------------------------
// /records/yearsago.json
// ---------------------------------------------------------------------------

#[test]
fn yearsago_request_matches_spec() {
    let mut h = Harness::new();
    h.mount(json!({"response": {"docs": []}}));
    h.ctx.limit = Some(5);

    let args = commands::yearsago::Args { years: 100 };
    commands::yearsago::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract("/records/yearsago.json", &["years", "number_show"]);
}

// ---------------------------------------------------------------------------
// /stats/archives.json
// ---------------------------------------------------------------------------

#[test]
fn archives_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!([]));

    commands::archives::run(&h.client, Some(&h.cache), &h.ctx).unwrap();
    h.assert_spec_contract("/stats/archives.json", &[]);
}

// ---------------------------------------------------------------------------
// /related/census.json
// ---------------------------------------------------------------------------

#[test]
fn census_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!({"place": "Amsterdam", "year": 1850}));

    let args = commands::census::Args {
        year: 1850,
        place: Some("Amsterdam".into()),
        gg_uri: None,
        province: Some("NH".into()),
        richness: Some(2),
    };
    commands::census::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/related/census.json",
        &["year", "place", "province", "richness"],
    );
}

// ---------------------------------------------------------------------------
// /related/weather.json
// ---------------------------------------------------------------------------

#[test]
fn weather_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!([]));

    let args = commands::weather::Args {
        date: "1898-04-15".into(),
        longitude: "4.9".into(),
        latitude: "52.37".into(),
    };
    commands::weather::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/related/weather.json",
        &["date", "longitude", "latitude", "lang"],
    );
}

// ---------------------------------------------------------------------------
// /stats/{records,sources,events,comments}.json — all share archive_stat.
// ---------------------------------------------------------------------------

#[test]
fn stats_records_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!([]));
    let args = ArchiveStatArgs {
        archive: Some("elo".into()),
    };
    commands::stats::records::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract("/stats/records.json", &["archive_code"]);
}

#[test]
fn stats_sources_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!([]));
    let args = ArchiveStatArgs {
        archive: Some("elo".into()),
    };
    commands::stats::sources::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract("/stats/sources.json", &["archive_code"]);
}

#[test]
fn stats_events_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!([]));
    let args = ArchiveStatArgs {
        archive: Some("elo".into()),
    };
    commands::stats::events::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract("/stats/events.json", &["archive_code"]);
}

#[test]
fn stats_comments_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!([]));
    let args = ArchiveStatArgs {
        archive: Some("elo".into()),
    };
    commands::stats::comments::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract("/stats/comments.json", &["archive_code"]);
}

// ---------------------------------------------------------------------------
// /stats/familynames.json
// ---------------------------------------------------------------------------

#[test]
fn stats_familynames_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!({"familynames": []}));
    let args = commands::stats::familynames::Args {
        place: Some("Amsterdam".into()),
        year_start: Some(1850),
        year_end: Some(1900),
        event_type: Some(1),
    };
    commands::stats::familynames::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/stats/familynames.json",
        &[
            "number_show",
            "lang",
            "eventplace",
            "eventyearstart",
            "eventyearend",
            "eventtype",
        ],
    );
}

// ---------------------------------------------------------------------------
// /stats/firstnames.json
// ---------------------------------------------------------------------------

#[test]
fn stats_firstnames_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!({"response": {"firstnames": []}}));
    let args = commands::stats::firstnames::Args {
        place: "Amsterdam".into(),
        year: 1850,
    };
    commands::stats::firstnames::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/stats/firstnames.json",
        &["eventplace", "eventyear", "number_show"],
    );
}

// ---------------------------------------------------------------------------
// /stats/professions.json
// ---------------------------------------------------------------------------

#[test]
fn stats_professions_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!({"professions": []}));
    let args = commands::stats::professions::Args {
        place: Some("Amsterdam".into()),
        year_start: Some(1850),
        year_end: Some(1900),
    };
    commands::stats::professions::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/stats/professions.json",
        &[
            "number_show",
            "lang",
            "eventplace",
            "eventyearstart",
            "eventyearend",
        ],
    );
}

// ---------------------------------------------------------------------------
// /stats/breakdown.json
// ---------------------------------------------------------------------------

#[test]
fn stats_breakdown_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!({
        "group_by": "archive",
        "total_records": 0,
        "total_groups": 0,
        "results": []
    }));
    let args = commands::stats::breakdown::Args {
        group_by: "archive".into(),
        archive: Some("elo".into()),
        source_type: Some("Bidprentjesverzameling".into()),
        event_type: Some(2),
        place: Some("Amsterdam".into()),
        year_start: Some(1700),
        year_end: Some(1800),
        min_count: Some(10),
        sort: Some("count_desc".into()),
    };
    commands::stats::breakdown::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/stats/breakdown.json",
        &[
            "group_by",
            "number_show",
            "lang",
            "archive_code",
            "sourcetype",
            "eventtype",
            "eventplace",
            "year_start",
            "year_end",
            "min_count",
            "sort",
        ],
    );
}

// ---------------------------------------------------------------------------
// /transcriptions/search.json
// ---------------------------------------------------------------------------

#[test]
fn transcripts_search_request_matches_spec() {
    let mut h = Harness::new();
    h.mount(json!({"response": {"docs": [], "numFound": 0}}));
    h.ctx.limit = Some(5);
    h.ctx.offset = Some(0);

    let args = commands::transcripts::search::Args {
        q: "coret".into(),
        archive_code: Some("hua".into()),
        archive_number: Some("123".into()),
        inventory_number: Some("99".into()),
        year_start: Some(1700),
        year_end: Some(1800),
    };
    commands::transcripts::search::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/transcriptions/search.json",
        &[
            "q",
            "number_show",
            "start",
            "lang",
            "archive_code",
            "archive_number",
            "inventory_number",
            "year_start",
            "year_end",
        ],
    );
}

// ---------------------------------------------------------------------------
// /transcriptions/browse.json
// ---------------------------------------------------------------------------

#[test]
fn transcripts_browse_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!({"response": {"docs": []}}));

    let args = commands::transcripts::browse::Args {
        archive_code: Some("hua".into()),
        archive_number: Some("123".into()),
    };
    commands::transcripts::browse::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract(
        "/transcriptions/browse.json",
        &["lang", "archive_code", "archive_number"],
    );
}

// ---------------------------------------------------------------------------
// /transcriptions/show.json
// ---------------------------------------------------------------------------

#[test]
fn transcripts_show_request_matches_spec() {
    let h = Harness::new();
    h.mount(json!({"id": "x", "text": "hello"}));

    let args = commands::transcripts::show::Args { id: "abc".into() };
    commands::transcripts::show::run(&h.client, Some(&h.cache), &h.ctx, &args).unwrap();
    h.assert_spec_contract("/transcriptions/show.json", &["id", "lang"]);
}
