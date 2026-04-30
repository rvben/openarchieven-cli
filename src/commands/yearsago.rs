use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 10;

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub years: u32,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    if ctx.offset.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--offset is not supported by `yearsago` (limit-only pagination)",
        ));
    }
    if ctx.lang != "nl" {
        return Err(Error::new(
            ErrorKind::Validation,
            "--lang is not supported by `yearsago`",
        ));
    }
    let limit = match ctx.limit {
        Some(n) if n > MAX_LIMIT => {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("--limit max is {MAX_LIMIT}, got {n}"),
            ));
        }
        Some(0) => {
            return Err(Error::new(
                ErrorKind::Validation,
                "--limit must be at least 1",
            ));
        }
        Some(n) => n,
        None => DEFAULT_LIMIT,
    };

    let years_s = args.years.to_string();
    let limit_s = limit.to_string();
    let params: Vec<(&str, &str)> = vec![
        ("yearsago", years_s.as_str()),
        ("number_show", limit_s.as_str()),
    ];

    let ttl = resolve_ttl(ctx, TtlHint::UntilMidnight);
    let body = client.get_cached("/records/yearsago.json", &params, ttl, cache)?;

    let items = body
        .pointer("/response/docs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let total = items.len() as u64;

    Ok(
        Renderable::list(serde_json::Value::Array(items), false, None, None)
            .with_total(Some(total)),
    )
}

pub fn schema() -> Command {
    Command {
        name: "yearsago",
        description: "Records from N years ago today (date-dependent).",
        mutating: false,
        response_shape: "list",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "until_midnight",
        args: vec![
            Arg {
                name: "years",
                ty: "integer",
                required: true,
                positional: true,
                default: None,
                min: Some(0),
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
                name: "paginated",
                ty: "boolean",
            },
        ],
    }
}
