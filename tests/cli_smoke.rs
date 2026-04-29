use assert_cmd::Command;
use predicates::str::contains;

fn bin() -> Command {
    Command::cargo_bin("openarchieven").unwrap()
}

#[test]
fn help_lists_top_level_commands() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("search"))
        .stdout(contains("show"))
        .stdout(contains("schema"))
        .stdout(contains("cache"));
}

#[test]
fn version_command_prints_version() {
    bin()
        .arg("version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn unknown_command_is_validation_error() {
    let out = bin().arg("nope").assert().failure();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("nope") || stderr.contains("error"));
}

#[test]
fn piped_stdout_defaults_to_json() {
    // assert_cmd captures stdout into a pipe, so this exercises the
    // non-TTY default path: schema must come out as JSON, not as a
    // human-formatted table.
    let out = bin().arg("schema").assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(
        stdout.trim_start().starts_with('{'),
        "expected JSON object, got: {}",
        &stdout[..stdout.len().min(120)]
    );
    let _: serde_json::Value =
        serde_json::from_str(&stdout).expect("piped stdout must be valid JSON");
}

#[test]
fn output_flag_overrides_pipe_default() {
    let dir = tempfile::tempdir().unwrap();
    let out = bin()
        .args(["--output", "markdown", "cache", "info"])
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    // cache info is single-flat; markdown should render as a bullet list.
    assert!(
        stdout.contains("- **entries**:"),
        "expected markdown bullet, got: {stdout}"
    );
}

#[test]
fn env_var_overrides_pipe_default() {
    let dir = tempfile::tempdir().unwrap();
    let out = bin()
        .env("OPENARCHIEVEN_OUTPUT", "markdown")
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .args(["cache", "info"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(
        stdout.contains("- **entries**:"),
        "expected markdown bullet, got: {stdout}"
    );
}
