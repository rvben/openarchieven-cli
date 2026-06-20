use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

pub const SUPPORTED_LANGS: &[&str] = &["nl", "en", "de", "fr"];
const TTL_SECONDS: u64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub archive_code: Option<String>,
    pub archive_number: Option<String>,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    if ctx.limit.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--limit is not supported by `transcripts browse` (non-paginated)",
        ));
    }
    if ctx.offset.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--offset is not supported by `transcripts browse` (non-paginated)",
        ));
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
    if args.archive_code.is_none() && args.archive_number.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--archive-number requires --archive-code",
        ));
    }

    let mut params: Vec<(&str, &str)> = vec![("lang", ctx.lang.as_str())];
    if let Some(ref v) = args.archive_code {
        params.push(("archive_code", v.as_str()));
    }
    if let Some(ref v) = args.archive_number {
        params.push(("archive_number", v.as_str()));
    }

    let ttl = resolve_ttl(ctx, default_ttl());
    let body = client.get_cached("/transcriptions/browse.json", &params, ttl, cache)?;

    let items = body
        .pointer("/response/docs")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let total = items.as_array().map(|a| a.len() as u64);

    Ok(Renderable::list(items, false, None, None).with_total(total))
}

fn default_ttl() -> TtlHint {
    TtlHint::Fixed(Duration::from_secs(TTL_SECONDS))
}

pub fn schema() -> Command {
    Command {
        name: "transcripts browse",
        description: "Hierarchical browse of available page transcriptions: archives → archive numbers → inventories.",
        mutating: false,
        response_shape: "list",
        paginated: false,
        cache_ttl_seconds: Some(TTL_SECONDS),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "--archive-code",
                ty: "string",
                required: false,
                positional: false,
                description: None,
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
                description: None,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--lang",
                ty: "string",
                required: false,
                positional: false,
                description: None,
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
                ty: "array<browse_item>",
                description: None,
            },
            OutputField {
                name: "total",
                ty: "integer",
                description: None,
            },
            OutputField {
                name: "limit",
                ty: "null",
                description: None,
            },
            OutputField {
                name: "offset",
                ty: "null",
                description: None,
            },
            OutputField {
                name: "paginated",
                ty: "boolean",
                description: None,
            },
        ],
    }
}
