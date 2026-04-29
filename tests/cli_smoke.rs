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
    let last = stderr.lines().last().expect("stderr must have a line");
    let v: serde_json::Value =
        serde_json::from_str(last).expect("last stderr line must be valid JSON");
    assert_eq!(v["error"]["kind"], "validation");
    assert_eq!(v["error"]["retryable"], false);
    assert_eq!(out.get_output().status.code(), Some(2));
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

#[test]
fn cache_clear_without_yes_emits_validation_json() {
    let dir = tempfile::tempdir().unwrap();
    let out = bin()
        .env("OPENARCHIEVEN_CACHE_DIR", dir.path())
        .args(["cache", "clear"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    let last = stderr.lines().last().expect("stderr must have a line");
    let v: serde_json::Value =
        serde_json::from_str(last).expect("last stderr line must be valid JSON");
    assert_eq!(v["error"]["kind"], "validation");
    assert!(
        v["error"]["message"].as_str().unwrap().contains("--yes"),
        "got message: {:?}",
        v["error"]["message"]
    );
}

#[test]
fn missing_required_arg_emits_validation_json() {
    // `show` requires a positional <id>. clap rejects, we wrap as validation.
    let out = bin().arg("show").assert().failure();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    let last = stderr.lines().last().expect("stderr must have a line");
    let v: serde_json::Value =
        serde_json::from_str(last).expect("last stderr line must be valid JSON");
    assert_eq!(v["error"]["kind"], "validation");
    assert_eq!(out.get_output().status.code(), Some(2));
}
