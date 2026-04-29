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
