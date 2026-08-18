//! Guards the contract the weekly drift workflow depends on.
//!
//! `.github/workflows/openapi-refresh.yml` branches on whether the vendored
//! OpenAPI spec still matches the live one. That signal travels on **stdout**
//! (`changed=true` / `changed=false`) with exit status 0 either way, because
//! `make` reports its own status 2 for any recipe failure and would flatten a
//! drift-signalling exit code into an indistinguishable error.
//!
//! These tests therefore run `make openapi-drift-status`, the exact command the
//! workflow runs and through the same make wrapper, rather than calling the
//! script directly. A return to exit-code signalling fails here first.
//!
//! `OPENARCHIEVEN_SPEC_URL` points the script at a local file, so nothing here
//! touches the live API. Running them needs `make` and `uv` on `PATH`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Run the drift target the way the workflow does, with the live spec source
/// redirected at `spec_url`.
fn drift_status(spec_url: &str) -> Output {
    Command::new("make")
        .arg("openapi-drift-status")
        .current_dir(repo_root())
        .env("OPENARCHIEVEN_SPEC_URL", spec_url)
        .output()
        .expect("`make` must be on PATH to run the drift-status tests")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

/// A live spec identical to the vendored one, by construction: the vendored
/// file *is* the source. Negative control for the drift case below.
#[test]
fn drift_status_reports_no_change_when_live_matches_vendored() {
    let vendored = repo_root().join("openapi/openarchieven.yaml");
    let out = drift_status(&file_url(&vendored));

    assert_eq!(
        out.status.code(),
        Some(0),
        "an up-to-date spec must exit 0\n  stdout: {}\n  stderr: {}",
        stdout(&out),
        stderr(&out)
    );
    assert_eq!(
        stdout(&out),
        "changed=false",
        "stdout is what the workflow appends to $GITHUB_OUTPUT; nothing else may \
         appear on it\n  stderr: {}",
        stderr(&out)
    );
}

/// The branch that was unreachable for the workflow's entire life: drift must
/// be reported as data on stdout, with a *successful* exit, so the refresh and
/// pull-request steps can run.
#[test]
fn drift_status_reports_change_and_still_exits_zero_when_live_differs() {
    let dir = tempdir().unwrap();
    let drifted = dir.path().join("drifted-openapi.yaml");
    std::fs::write(&drifted, "openapi: 3.0.3\npaths: {}\n").unwrap();

    let out = drift_status(&file_url(&drifted));

    assert_eq!(
        out.status.code(),
        Some(0),
        "drift is an outcome, not a failure: signalling it by exiting non-zero \
         is flattened to make's status 2 and hard-fails the workflow instead of \
         opening a refresh PR\n  stdout: {}\n  stderr: {}",
        stdout(&out),
        stderr(&out)
    );
    assert_eq!(stdout(&out), "changed=true", "stderr: {}", stderr(&out));
}

/// An unreachable spec is neither "up to date" nor "drifted". It must stay
/// distinguishable from both, or a broken fetch silently reads as no-drift and
/// the drift check becomes a guard that can never fire.
#[test]
fn drift_status_fails_loudly_when_the_spec_cannot_be_fetched() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("no-such-spec.yaml");

    let out = drift_status(&file_url(&missing));

    assert_ne!(
        out.status.code(),
        Some(0),
        "an unreachable spec must fail the step\n  stdout: {}\n  stderr: {}",
        stdout(&out),
        stderr(&out)
    );
    assert!(
        !stdout(&out).contains("changed="),
        "a fetch failure must not emit a drift verdict on stdout, got: {}",
        stdout(&out)
    );
}
