use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

const MIN_YEAR: i32 = 1600;
const MAX_YEAR: i32 = 1960;
const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 20;

#[derive(Debug, Clone)]
pub struct Args {
    pub place: String,
    pub year: i32,
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
            "--offset not supported by `stats firstnames`",
        ));
    }
    if ctx.lang != "nl" {
        return Err(Error::new(
            ErrorKind::Validation,
            "--lang not supported by `stats firstnames`",
        ));
    }
    if !(MIN_YEAR..=MAX_YEAR).contains(&args.year) {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--year must be {MIN_YEAR}..={MAX_YEAR}, got {}", args.year),
        ));
    }
    let limit = match ctx.limit {
        Some(0) => {
            return Err(Error::new(ErrorKind::Validation, "--limit must be >= 1"));
        }
        Some(n) if n > MAX_LIMIT => {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("--limit max is {MAX_LIMIT}, got {n}"),
            ));
        }
        Some(n) => n,
        None => DEFAULT_LIMIT,
    };

    let yr = args.year.to_string();
    let limit_s = limit.to_string();
    let params: Vec<(&str, &str)> = vec![
        ("eventplace", args.place.as_str()),
        ("eventyear", yr.as_str()),
        ("number_show", limit_s.as_str()),
    ];

    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(24 * 3600)));
    let body = client.get_cached("/stats/firstnames.json", &params, ttl, cache)?;
    let items = body
        .pointer("/response/firstnames")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let total = items.len() as u64;
    Ok(
        Renderable::list(serde_json::Value::Array(items), false, Some(limit), None)
            .with_total(Some(total)),
    )
}

pub fn schema() -> Command {
    Command {
        name: "stats firstnames",
        description: "First-name frequency stats for a place and year.",
        mutating: false,
        response_shape: "list",
        paginated: false,
        cache_ttl_seconds: Some(24 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "--place",
                ty: "string",
                required: true,
                positional: false,
                description: None,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--year",
                ty: "integer",
                required: true,
                positional: false,
                description: None,
                default: None,
                min: Some(MIN_YEAR as i64),
                max: Some(MAX_YEAR as i64),
                r#enum: None,
            },
            Arg {
                name: "--limit",
                ty: "integer",
                required: false,
                positional: false,
                description: None,
                default: Some(serde_json::json!(DEFAULT_LIMIT)),
                min: Some(1),
                max: Some(MAX_LIMIT as i64),
                r#enum: None,
            },
        ],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<row>",
                description: None,
            },
            OutputField {
                name: "total",
                ty: "integer",
                description: None,
            },
            OutputField {
                name: "limit",
                ty: "integer",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_returns_correct_command_name() {
        let cmd = schema();
        assert_eq!(cmd.name, "stats firstnames");
        let place = cmd.args.iter().find(|a| a.name == "--place").unwrap();
        assert!(place.required);
        let year = cmd.args.iter().find(|a| a.name == "--year").unwrap();
        assert_eq!(year.min, Some(MIN_YEAR as i64));
        assert_eq!(year.max, Some(MAX_YEAR as i64));
    }
}
