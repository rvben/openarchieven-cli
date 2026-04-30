use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

pub const SUPPORTED_LANGS: &[&str] = &["nl", "en"];
pub const MAX_LIMIT: u32 = 100;
pub const DEFAULT_LIMIT: u32 = 10;

const SUPPORTED_FLAGS: &[&str] = &[
    "--archive",
    "--source-type",
    "--event-place",
    "--birth-place",
    "--relation-type",
    "--country",
    "--sort",
];

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub name: String,
    pub archive: Option<String>,
    pub source_type: Option<String>,
    pub event_place: Option<String>,
    pub birth_place: Option<String>,
    pub relation_type: Option<String>,
    pub country: Option<String>,
    pub sort: Option<i32>,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    if !SUPPORTED_LANGS.contains(&ctx.lang.as_str()) {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "--lang: unsupported language '{}', supported: {}",
                ctx.lang,
                SUPPORTED_LANGS.join(", ")
            ),
        ));
    }

    let limit = ctx.limit.unwrap_or(DEFAULT_LIMIT);
    if limit > MAX_LIMIT {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--limit: exceeds maximum of {MAX_LIMIT}"),
        ));
    }
    if limit == 0 {
        return Err(Error::new(
            ErrorKind::Validation,
            "--limit: must be at least 1",
        ));
    }

    let offset = ctx.offset.unwrap_or(0);

    let limit_str = limit.to_string();
    let start_str = offset.to_string();
    let mut params: Vec<(&str, &str)> = vec![
        ("name", args.name.as_str()),
        ("number_show", &limit_str),
        ("start", &start_str),
        ("lang", ctx.lang.as_str()),
    ];

    if let Some(ref v) = args.archive {
        params.push(("archive_code", v.as_str()));
    }
    if let Some(ref v) = args.source_type {
        params.push(("source_type", v.as_str()));
    }
    if let Some(ref v) = args.event_place {
        params.push(("event_place", v.as_str()));
    }
    if let Some(ref v) = args.birth_place {
        params.push(("birth_place", v.as_str()));
    }
    if let Some(ref v) = args.relation_type {
        params.push(("relation_type", v.as_str()));
    }
    if let Some(ref v) = args.country {
        params.push(("country", v.as_str()));
    }
    let sort_str;
    if let Some(s) = args.sort {
        sort_str = s.to_string();
        params.push(("sort", &sort_str));
    }

    let ttl = resolve_ttl(ctx, default_ttl());
    let body = client.get_cached("/records/search.json", &params, ttl, cache)?;

    let items = body
        .pointer("/response/docs")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    let total = body.pointer("/response/numFound").and_then(|v| v.as_u64());

    Ok(Renderable::list(items, true, Some(limit), Some(offset)).with_total(total))
}

pub fn parse_rest(rest: &[String]) -> Result<Args> {
    let mut a = Args::default();
    let mut positionals: Vec<String> = Vec::new();
    let mut iter = rest.iter();

    while let Some(tok) = iter.next() {
        let s = tok.as_str();
        if let Some(v) = s.strip_prefix("--archive=") {
            a.archive = Some(non_empty("--archive", v)?);
        } else if s == "--archive" {
            a.archive = Some(next_value("--archive", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--source-type=") {
            a.source_type = Some(non_empty("--source-type", v)?);
        } else if s == "--source-type" {
            a.source_type = Some(next_value("--source-type", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--event-place=") {
            a.event_place = Some(non_empty("--event-place", v)?);
        } else if s == "--event-place" {
            a.event_place = Some(next_value("--event-place", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--birth-place=") {
            a.birth_place = Some(non_empty("--birth-place", v)?);
        } else if s == "--birth-place" {
            a.birth_place = Some(next_value("--birth-place", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--relation-type=") {
            a.relation_type = Some(non_empty("--relation-type", v)?);
        } else if s == "--relation-type" {
            a.relation_type = Some(next_value("--relation-type", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--country=") {
            a.country = Some(non_empty("--country", v)?);
        } else if s == "--country" {
            a.country = Some(next_value("--country", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--sort=") {
            a.sort = Some(parse_sort(v)?);
        } else if s == "--sort" {
            let v = next_value("--sort", &mut iter)?;
            a.sort = Some(parse_sort(&v)?);
        } else if s.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!(
                    "unknown flag: {s}. supported: {}",
                    SUPPORTED_FLAGS.join(", ")
                ),
            ));
        } else {
            positionals.push(tok.clone());
        }
    }

    if positionals.len() > 1 {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "search: expected exactly one positional argument (name), got: {}",
                positionals.join(", ")
            ),
        ));
    }

    a.name = positionals.into_iter().next().ok_or_else(|| {
        Error::new(
            ErrorKind::Validation,
            "search: missing required argument: name",
        )
    })?;

    Ok(a)
}

fn next_value(flag: &str, iter: &mut std::slice::Iter<'_, String>) -> Result<String> {
    match iter.next() {
        Some(v) if v.starts_with("--") => Err(Error::new(
            ErrorKind::Validation,
            format!("{flag}: missing value (got flag '{v}' instead)"),
        )),
        Some(v) => Ok(v.clone()),
        None => Err(Error::new(
            ErrorKind::Validation,
            format!("{flag}: missing value"),
        )),
    }
}

fn non_empty(flag: &str, v: &str) -> Result<String> {
    if v.is_empty() {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("{flag}: value must not be empty"),
        ));
    }
    Ok(v.to_string())
}

/// Validate `--sort` per the schema enum: integers in [-6,-1] ∪ [1,6].
/// 0 is rejected; values outside the range are rejected.
fn parse_sort(v: &str) -> Result<i32> {
    let n: i32 = v.parse().map_err(|_| {
        Error::new(
            ErrorKind::Validation,
            format!("--sort: not an integer: {v}"),
        )
    })?;
    if n == 0 || !(-6..=6).contains(&n) {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--sort: must be in -6..=-1 or 1..=6, got {n}"),
        ));
    }
    Ok(n)
}

fn default_ttl() -> TtlHint {
    TtlHint::Fixed(Duration::from_secs(6 * 3600))
}

pub fn schema() -> Command {
    Command {
        name: "search",
        description: "Free-text record search across all archives.",
        mutating: false,
        response_shape: "list",
        paginated: true,
        cache_ttl_seconds: Some(6 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "name",
                ty: "string",
                required: true,
                positional: true,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--archive",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--source-type",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--event-place",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--birth-place",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--relation-type",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--country",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--sort",
                ty: "integer",
                required: false,
                positional: false,
                default: Some(serde_json::json!(1)),
                min: None,
                max: None,
                r#enum: Some(vec![
                    serde_json::json!(-6),
                    serde_json::json!(-5),
                    serde_json::json!(-4),
                    serde_json::json!(-3),
                    serde_json::json!(-2),
                    serde_json::json!(-1),
                    serde_json::json!(1),
                    serde_json::json!(2),
                    serde_json::json!(3),
                    serde_json::json!(4),
                    serde_json::json!(5),
                    serde_json::json!(6),
                ]),
            },
            Arg {
                name: "--limit",
                ty: "integer",
                required: false,
                positional: false,
                default: Some(serde_json::json!(10)),
                min: None,
                max: Some(100),
                r#enum: None,
            },
            Arg {
                name: "--offset",
                ty: "integer",
                required: false,
                positional: false,
                default: Some(serde_json::json!(0)),
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--lang",
                ty: "string",
                required: false,
                positional: false,
                default: Some(serde_json::json!("nl")),
                min: None,
                max: None,
                r#enum: Some(vec![serde_json::json!("nl"), serde_json::json!("en")]),
            },
        ],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<record>",
            },
            OutputField {
                name: "total",
                ty: "integer | null",
            },
            OutputField {
                name: "limit",
                ty: "integer",
            },
            OutputField {
                name: "offset",
                ty: "integer",
            },
            OutputField {
                name: "paginated",
                ty: "boolean",
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn missing_name_is_validation_error() {
        let err = parse_rest(&[]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("name"), "message: {}", err.message());
    }

    #[test]
    fn multiple_positionals_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "de groot"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn unknown_flag_lists_supported() {
        let err = parse_rest(&strs(&["--zzz", "jansen"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("--zzz"),
            "message: {}",
            err.message()
        );
        assert!(
            err.message().contains("--archive"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn archive_space_form() {
        let a = parse_rest(&strs(&["--archive", "elo", "jansen"])).unwrap();
        assert_eq!(a.archive.as_deref(), Some("elo"));
        assert_eq!(a.name, "jansen");
    }

    #[test]
    fn archive_eq_form() {
        let a = parse_rest(&strs(&["--archive=elo", "jansen"])).unwrap();
        assert_eq!(a.archive.as_deref(), Some("elo"));
    }

    #[test]
    fn archive_eq_empty_is_validation_error() {
        let err = parse_rest(&strs(&["--archive=", "jansen"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("--archive"),
            "message: {}",
            err.message()
        );
        assert!(
            err.message().contains("empty"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn archive_at_end_of_input_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "--archive"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("--archive"),
            "message: {}",
            err.message()
        );
        assert!(
            err.message().contains("missing value"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn archive_followed_by_flag_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "--archive", "--sort", "3"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("--archive"),
            "message: {}",
            err.message()
        );
        assert!(
            err.message().contains("--sort"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn sort_space_form() {
        let a = parse_rest(&strs(&["jansen", "--sort", "3"])).unwrap();
        assert_eq!(a.sort, Some(3));
    }

    #[test]
    fn sort_negative_value() {
        let a = parse_rest(&strs(&["jansen", "--sort", "-3"])).unwrap();
        assert_eq!(a.sort, Some(-3));
    }

    #[test]
    fn sort_not_an_integer() {
        let err = parse_rest(&strs(&["jansen", "--sort=notanint"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("--sort"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn sort_zero_is_rejected() {
        let err = parse_rest(&strs(&["jansen", "--sort=0"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("-6..=-1 or 1..=6"));
    }

    #[test]
    fn sort_above_six_is_rejected() {
        let err = parse_rest(&strs(&["jansen", "--sort", "7"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn sort_below_negative_six_is_rejected() {
        let err = parse_rest(&strs(&["jansen", "--sort=-7"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn full_args_roundtrip() {
        let a = parse_rest(&strs(&[
            "jansen",
            "--archive",
            "elo",
            "--source-type",
            "BS",
            "--event-place=Amsterdam",
            "--birth-place=Leiden",
            "--relation-type=vader",
            "--country=NL",
            "--sort=2",
        ]))
        .unwrap();
        assert_eq!(a.name, "jansen");
        assert_eq!(a.archive.as_deref(), Some("elo"));
        assert_eq!(a.source_type.as_deref(), Some("BS"));
        assert_eq!(a.event_place.as_deref(), Some("Amsterdam"));
        assert_eq!(a.birth_place.as_deref(), Some("Leiden"));
        assert_eq!(a.relation_type.as_deref(), Some("vader"));
        assert_eq!(a.country.as_deref(), Some("NL"));
        assert_eq!(a.sort, Some(2));
    }

    #[test]
    fn source_type_space_form() {
        let a = parse_rest(&strs(&["jansen", "--source-type", "BS"])).unwrap();
        assert_eq!(a.source_type.as_deref(), Some("BS"));
    }

    #[test]
    fn source_type_eq_empty_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "--source-type="])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("--source-type"),
            "msg: {}",
            err.message()
        );
    }

    #[test]
    fn event_place_space_form() {
        let a = parse_rest(&strs(&["jansen", "--event-place", "Amsterdam"])).unwrap();
        assert_eq!(a.event_place.as_deref(), Some("Amsterdam"));
    }

    #[test]
    fn event_place_eq_empty_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "--event-place="])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn birth_place_space_form() {
        let a = parse_rest(&strs(&["jansen", "--birth-place", "Leiden"])).unwrap();
        assert_eq!(a.birth_place.as_deref(), Some("Leiden"));
    }

    #[test]
    fn birth_place_eq_empty_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "--birth-place="])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn relation_type_space_form() {
        let a = parse_rest(&strs(&["jansen", "--relation-type", "vader"])).unwrap();
        assert_eq!(a.relation_type.as_deref(), Some("vader"));
    }

    #[test]
    fn relation_type_eq_empty_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "--relation-type="])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn country_space_form() {
        let a = parse_rest(&strs(&["jansen", "--country", "NL"])).unwrap();
        assert_eq!(a.country.as_deref(), Some("NL"));
    }

    #[test]
    fn country_eq_empty_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "--country="])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn sort_at_end_of_input_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "--sort"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--sort"), "msg: {}", err.message());
    }

    #[test]
    fn sort_followed_by_flag_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "--sort", "--archive"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn sort_extreme_valid_values_accepted() {
        let a = parse_rest(&strs(&["jansen", "--sort", "-6"])).unwrap();
        assert_eq!(a.sort, Some(-6));
        let a = parse_rest(&strs(&["jansen", "--sort", "6"])).unwrap();
        assert_eq!(a.sort, Some(6));
    }

    #[test]
    fn schema_returns_correct_command_name() {
        let cmd = schema();
        assert_eq!(cmd.name, "search");
        assert!(cmd.paginated);
        assert_eq!(cmd.response_shape, "list");
        let sort_arg = cmd.args.iter().find(|a| a.name == "--sort").unwrap();
        assert!(sort_arg.r#enum.is_some());
        assert_eq!(sort_arg.r#enum.as_ref().unwrap().len(), 12);
        let name_arg = cmd.args.iter().find(|a| a.name == "name").unwrap();
        assert!(name_arg.required);
        assert!(name_arg.positional);
        let limit_arg = cmd.args.iter().find(|a| a.name == "--limit").unwrap();
        assert_eq!(limit_arg.max, Some(100));
        let lang_arg = cmd.args.iter().find(|a| a.name == "--lang").unwrap();
        assert!(lang_arg.r#enum.is_some());
        let archive_arg = cmd.args.iter().find(|a| a.name == "--archive").unwrap();
        assert!(!archive_arg.required);
        let source_type = cmd.args.iter().find(|a| a.name == "--source-type").unwrap();
        assert!(!source_type.required);
        let event_place = cmd.args.iter().find(|a| a.name == "--event-place").unwrap();
        assert!(!event_place.required);
        let birth_place = cmd.args.iter().find(|a| a.name == "--birth-place").unwrap();
        assert!(!birth_place.required);
        let relation_type = cmd
            .args
            .iter()
            .find(|a| a.name == "--relation-type")
            .unwrap();
        assert!(!relation_type.required);
        let country = cmd.args.iter().find(|a| a.name == "--country").unwrap();
        assert!(!country.required);
        assert_eq!(cmd.output_fields.len(), 5);
        assert!(cmd.output_fields.iter().any(|f| f.name == "items"));
        assert!(cmd.output_fields.iter().any(|f| f.name == "total"));
    }
}
