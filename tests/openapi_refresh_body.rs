//! Guards the pull-request body that `make openapi-refresh` writes.
//!
//! The weekly workflow refreshes the vendored spec and opens a PR asking a human
//! to confirm the change is intentional. A full OpenAPI yaml diff buries that
//! question, so the refresh derives a summary of how the *manifest* changed and
//! the PR leads with it.
//!
//! These tests drive the real `make openapi-refresh`, the same target the
//! workflow runs. `OPENARCHIEVEN_SPEC_URL` points the script at a local fixture
//! and `OPENARCHIEVEN_OPENAPI_DIR` redirects its output, so nothing here touches
//! the network or the vendored files. Running them needs `make` and `uv` on
//! `PATH`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::{TempDir, tempdir};

/// A spec that, compared against `BASELINE_MANIFEST`, exercises all three kinds
/// of manifest change at once: `/records/brandnew.json` is new,
/// `/legacy/gone.json` has disappeared, and `/records/search.json` gained `sort`
/// while losing `lang`.
const FIXTURE_SPEC: &str = r#"openapi: 3.0.3
info:
  title: fixture
  version: "1"
paths:
  /records/search.json:
    get:
      operationId: search
      parameters:
        - name: name
          in: query
          schema: { type: string }
        - name: sort
          in: query
          schema: { type: integer }
  /records/brandnew.json:
    get:
      operationId: brandNew
      parameters:
        - name: q
          in: query
          schema: { type: string }
"#;

const BASELINE_MANIFEST: &str = r#"{
  "/legacy/gone.json": {
    "method": "GET",
    "operationId": "gone",
    "query_params": ["x"]
  },
  "/records/search.json": {
    "method": "GET",
    "operationId": "search",
    "query_params": ["lang", "name"]
  }
}
"#;

/// The manifest `FIXTURE_SPEC` produces, so a refresh against it is a no-op.
const UNCHANGED_MANIFEST: &str = r#"{
  "/records/brandnew.json": {
    "method": "GET",
    "operationId": "brandNew",
    "query_params": ["q"]
  },
  "/records/search.json": {
    "method": "GET",
    "operationId": "search",
    "query_params": ["name", "sort"]
  }
}
"#;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

struct Refresh {
    /// Owns the temp dir; dropping it deletes `openapi_dir`.
    _dir: TempDir,
    openapi_dir: PathBuf,
    body: String,
}

/// Run `make openapi-refresh` against `FIXTURE_SPEC`, with `baseline` seeded as
/// the previously committed manifest (or nothing seeded, for a first refresh).
///
/// `MAKEFLAGS` and friends are stripped for the same reason as in
/// `tests/openapi_drift.rs`: `make check` reaches these tests through cargo, and
/// a sub-make would announce itself in the captured output.
fn refresh(baseline: Option<&str>) -> Refresh {
    let dir = tempdir().unwrap();
    let openapi_dir = dir.path().join("openapi");
    fs::create_dir_all(&openapi_dir).unwrap();
    if let Some(baseline) = baseline {
        fs::write(openapi_dir.join("params-manifest.json"), baseline).unwrap();
    }
    let spec_path = dir.path().join("fixture-openapi.yaml");
    fs::write(&spec_path, FIXTURE_SPEC).unwrap();
    let body_path = dir.path().join("body.md");

    let out = Command::new("make")
        .arg("openapi-refresh")
        .current_dir(repo_root())
        .env("OPENARCHIEVEN_SPEC_URL", file_url(&spec_path))
        .env("OPENARCHIEVEN_OPENAPI_DIR", &openapi_dir)
        .env("OPENARCHIEVEN_BODY_PATH", &body_path)
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .env_remove("MAKELEVEL")
        .output()
        .expect("`make` must be on PATH to run the refresh-body tests");

    assert!(
        out.status.success(),
        "make openapi-refresh failed ({:?})\n  stdout: {}\n  stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let body = fs::read_to_string(&body_path).unwrap_or_else(|e| {
        panic!(
            "--refresh must write a pull-request body to {}: {e}\n  stdout: {}",
            body_path.display(),
            String::from_utf8_lossy(&out.stdout)
        )
    });

    Refresh {
        _dir: dir,
        openapi_dir,
        body,
    }
}

/// The lines under `heading`, up to the next heading of any level. `None` when
/// the heading is absent, so a test can assert a section is missing without an
/// empty string passing for "present but empty".
fn section(body: &str, heading: &str) -> Option<String> {
    let mut inside = false;
    let mut lines = Vec::new();
    for line in body.lines() {
        if line.trim() == heading {
            inside = true;
            continue;
        }
        if inside && line.starts_with('#') {
            break;
        }
        if inside {
            lines.push(line);
        }
    }
    inside.then(|| lines.join("\n").trim().to_string())
}

fn require_section(body: &str, heading: &str) -> String {
    section(body, heading)
        .unwrap_or_else(|| panic!("body has no `{heading}` section\n--- body ---\n{body}"))
}

/// Every kind of manifest change reaches the body, attached to the right
/// endpoint. Asserting whole sections rather than loose substrings catches a
/// param summarised under the wrong path.
#[test]
fn refresh_body_reports_added_removed_and_changed_endpoints() {
    let r = refresh(Some(BASELINE_MANIFEST));

    assert_eq!(
        require_section(&r.body, "### New endpoints"),
        "- `/records/brandnew.json` (`GET` `brandNew`) query params: `q`",
        "--- body ---\n{}",
        r.body
    );
    assert_eq!(
        require_section(&r.body, "### Removed endpoints"),
        "- `/legacy/gone.json` (was `GET` `gone`)",
        "--- body ---\n{}",
        r.body
    );
    assert_eq!(
        require_section(&r.body, "### Changed endpoints"),
        "- `/records/search.json`\n  - added query params: `sort`\n  - removed query params: `lang`",
        "--- body ---\n{}",
        r.body
    );

    let checks = require_section(&r.body, "## What to check");
    assert!(
        checks.contains("INTENTIONALLY_UNWRAPPED"),
        "an added endpoint must raise the unwrapped-endpoint question\n  checks: {checks}"
    );
    assert!(
        checks.contains("newly added optional parameter"),
        "an added parameter must raise the unwrapped-parameter question\n  checks: {checks}"
    );
}

/// A spec can drift in bytes while the manifest stays identical, because the
/// manifest tracks only paths, methods and query-parameter names. Saying so
/// plainly is the point: it tells the reviewer the change is confined to parts
/// no contract test can see, which is precisely when reading the diff matters.
#[test]
fn refresh_body_reports_an_unchanged_manifest_as_unchanged() {
    let r = refresh(Some(UNCHANGED_MANIFEST));

    for heading in [
        "### New endpoints",
        "### Removed endpoints",
        "### Changed endpoints",
    ] {
        assert!(
            section(&r.body, heading).is_none(),
            "an unchanged manifest must not render a `{heading}` section\n--- body ---\n{}",
            r.body
        );
    }
    assert!(
        r.body.contains("manifest is unchanged"),
        "the body must state that the manifest did not change\n--- body ---\n{}",
        r.body
    );

    let checks = require_section(&r.body, "## What to check");
    assert!(
        !checks.contains("INTENTIONALLY_UNWRAPPED"),
        "with no endpoint added there is no unwrapped endpoint to ask about\n  checks: {checks}"
    );
    assert!(
        checks.contains("manifest records paths"),
        "the blind-spot caveat holds for every refresh and must always appear\n  checks: {checks}"
    );
}

/// A first refresh has nothing to compare against. "No baseline" is not "every
/// endpoint is new": rendering 21 endpoints as additions would be a confident
/// wrong answer, and the reviewer would learn nothing from it.
#[test]
fn refresh_body_does_not_present_a_missing_baseline_as_new_endpoints() {
    let r = refresh(None);

    assert!(
        section(&r.body, "### New endpoints").is_none(),
        "with no previous manifest there is nothing to call new\n--- body ---\n{}",
        r.body
    );
    assert!(
        r.body.contains("no previous manifest"),
        "the body must say why there is no comparison\n--- body ---\n{}",
        r.body
    );
}

/// The output-directory override is what keeps these tests from overwriting the
/// repository's own vendored spec. If it silently stopped taking effect, every
/// run of the suite would clobber `openapi/`.
#[test]
fn refresh_writes_its_artifacts_into_the_override_directory() {
    let r = refresh(Some(BASELINE_MANIFEST));

    let spec = fs::read_to_string(r.openapi_dir.join("openarchieven.yaml"))
        .expect("refreshed spec lands in the override directory");
    assert_eq!(spec, FIXTURE_SPEC, "the fetched spec is written verbatim");

    let sha = fs::read_to_string(r.openapi_dir.join("openarchieven.sha256"))
        .expect("refreshed sha lands in the override directory");
    let sha = sha.trim();
    assert!(
        sha.len() == 64 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "expected a sha256 hex digest, got {sha:?}"
    );

    let manifest = fs::read_to_string(r.openapi_dir.join("params-manifest.json"))
        .expect("regenerated manifest lands in the override directory");
    assert!(
        manifest.contains("/records/brandnew.json"),
        "the manifest is rebuilt from the fixture spec, got: {manifest}"
    );
}
