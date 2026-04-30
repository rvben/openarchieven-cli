use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

pub const SUPPORTED_LANGS: &[&str] = &["nl", "en", "de", "fr"];
pub const MAX_LIMIT: u32 = 100;
pub const DEFAULT_LIMIT: u32 = 10;

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub q: String,
    pub archive_code: Option<String>,
    pub archive_number: Option<String>,
    pub inventory_number: Option<String>,
    pub year_start: Option<i32>,
    pub year_end: Option<i32>,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    if args.q.is_empty() {
        return Err(Error::new(ErrorKind::Validation, "q: must not be empty"));
    }
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

    let limit = match ctx.limit {
        Some(0) => {
            return Err(Error::new(
                ErrorKind::Validation,
                "--limit: must be at least 1",
            ));
        }
        Some(n) if n > MAX_LIMIT => {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("--limit: exceeds maximum of {MAX_LIMIT}"),
            ));
        }
        Some(n) => n,
        None => DEFAULT_LIMIT,
    };
    let offset = ctx.offset.unwrap_or(0);

    let limit_str = limit.to_string();
    let start_str = offset.to_string();
    let year_start_str;
    let year_end_str;
    let mut params: Vec<(&str, &str)> = vec![
        ("q", args.q.as_str()),
        ("number_show", &limit_str),
        ("start", &start_str),
        ("lang", ctx.lang.as_str()),
    ];
    if let Some(ref v) = args.archive_code {
        params.push(("archive_code", v.as_str()));
    }
    if let Some(ref v) = args.archive_number {
        params.push(("archive_number", v.as_str()));
    }
    if let Some(ref v) = args.inventory_number {
        params.push(("inventory_number", v.as_str()));
    }
    if let Some(y) = args.year_start {
        year_start_str = y.to_string();
        params.push(("year_start", &year_start_str));
    }
    if let Some(y) = args.year_end {
        year_end_str = y.to_string();
        params.push(("year_end", &year_end_str));
    }

    let ttl = resolve_ttl(ctx, default_ttl());
    let body = client.get_cached("/transcriptions/search.json", &params, ttl, cache)?;

    let items = body
        .pointer("/response/docs")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let total = body
        .pointer("/response/number_found")
        .and_then(|v| v.as_u64());

    Ok(Renderable::list(items, true, Some(limit), Some(offset)).with_total(total))
}

fn default_ttl() -> TtlHint {
    TtlHint::Fixed(Duration::from_secs(6 * 3600))
}

pub fn schema() -> Command {
    Command {
        name: "transcripts search",
        description: "Full-text search across page transcriptions of historical documents.",
        mutating: false,
        response_shape: "list",
        paginated: true,
        cache_ttl_seconds: Some(6 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "q",
                ty: "string",
                required: true,
                positional: true,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--archive-code",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--archive-number",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--inventory-number",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--year-start",
                ty: "integer",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--year-end",
                ty: "integer",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--limit",
                ty: "integer",
                required: false,
                positional: false,
                default: Some(serde_json::json!(10)),
                min: Some(1),
                max: Some(100),
                r#enum: None,
            },
            Arg {
                name: "--offset",
                ty: "integer",
                required: false,
                positional: false,
                default: Some(serde_json::json!(0)),
                min: Some(0),
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
                r#enum: Some(vec![
                    serde_json::json!("nl"),
                    serde_json::json!("en"),
                    serde_json::json!("de"),
                    serde_json::json!("fr"),
                ]),
            },
        ],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<transcription_match>",
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
