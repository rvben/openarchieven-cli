use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, TtlOverride};
use crate::schema_cmd::{Arg, Command, OutputField};

pub const SUPPORTED_LANGS: &[&str] = &["nl", "en"];
pub const MAX_LIMIT: u32 = 100;
pub const DEFAULT_LIMIT: u32 = 10;

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
    let supported_flags = [
        "--archive",
        "--source-type",
        "--event-place",
        "--birth-place",
        "--relation-type",
        "--country",
        "--sort",
    ];

    let mut a = Args {
        name: String::new(),
        archive: None,
        source_type: None,
        event_place: None,
        birth_place: None,
        relation_type: None,
        country: None,
        sort: None,
    };

    let mut positionals: Vec<String> = Vec::new();
    let mut iter = rest.iter();

    while let Some(tok) = iter.next() {
        let s = tok.as_str();
        if let Some(v) = s.strip_prefix("--archive=") {
            a.archive = Some(v.to_string());
        } else if s == "--archive" {
            a.archive = Some(next_value("--archive", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--source-type=") {
            a.source_type = Some(v.to_string());
        } else if s == "--source-type" {
            a.source_type = Some(next_value("--source-type", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--event-place=") {
            a.event_place = Some(v.to_string());
        } else if s == "--event-place" {
            a.event_place = Some(next_value("--event-place", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--birth-place=") {
            a.birth_place = Some(v.to_string());
        } else if s == "--birth-place" {
            a.birth_place = Some(next_value("--birth-place", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--relation-type=") {
            a.relation_type = Some(v.to_string());
        } else if s == "--relation-type" {
            a.relation_type = Some(next_value("--relation-type", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--country=") {
            a.country = Some(v.to_string());
        } else if s == "--country" {
            a.country = Some(next_value("--country", &mut iter)?);
        } else if let Some(v) = s.strip_prefix("--sort=") {
            a.sort = Some(v.parse::<i32>().map_err(|_| {
                Error::new(
                    ErrorKind::Validation,
                    format!("--sort: not an integer: {v}"),
                )
            })?);
        } else if s == "--sort" {
            let v = next_value("--sort", &mut iter)?;
            a.sort = Some(v.parse::<i32>().map_err(|_| {
                Error::new(
                    ErrorKind::Validation,
                    format!("--sort: not an integer: {v}"),
                )
            })?);
        } else if s.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!(
                    "unknown flag: {s}. supported: {}",
                    supported_flags.join(", ")
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
    iter.next()
        .cloned()
        .ok_or_else(|| Error::new(ErrorKind::Validation, format!("{flag}: missing value")))
}

fn default_ttl() -> TtlHint {
    TtlHint::Fixed(Duration::from_secs(6 * 3600))
}

fn resolve_ttl(ctx: &ApiContext, default: TtlHint) -> TtlHint {
    match ctx.cache_ttl_override {
        Some(TtlOverride::Disabled) => TtlHint::None,
        Some(TtlOverride::Forever) => TtlHint::Never,
        Some(TtlOverride::Fixed(d)) => TtlHint::Fixed(d),
        None => default,
    }
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
                ty: "integer",
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
