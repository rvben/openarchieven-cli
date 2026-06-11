//! Byte-stability snapshot for the `schema` document.
//!
//! Any change to argument shapes, output fields, error kinds, or the
//! command list updates this snapshot — review the diff to make sure the
//! contract change is intentional before accepting it.

use openarchieven::schema_cmd;

#[test]
fn schema_is_byte_stable() {
    let s = schema_cmd::build();
    insta::assert_json_snapshot!(
        s,
        { ".version" => "[version]" }
    );
}

#[test]
fn schema_command_list_is_complete() {
    let s = schema_cmd::build();
    let names: Vec<&str> = s.commands.iter().map(|c| c.name).collect();

    let expected = [
        "archives",
        "search",
        "show",
        "match",
        "births",
        "deaths",
        "marriages",
        "yearsago",
        "census",
        "weather",
        "stats records",
        "stats sources",
        "stats events",
        "stats comments",
        "stats familynames",
        "stats firstnames",
        "stats professions",
        "stats breakdown",
        "transcripts search",
        "transcripts browse",
        "transcripts show",
        "cache info",
        "cache clear",
        "cache prune",
        "schema",
        "version",
    ];
    assert_eq!(names, expected);
}

#[test]
fn schema_error_kinds_match_runtime_enum() {
    let s = schema_cmd::build();
    let kinds: Vec<&str> = s.errors.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            "validation",
            "not_found",
            "rate_limit",
            "timeout",
            "network",
            "server",
            "parse",
            "conflict",
        ]
    );
}
