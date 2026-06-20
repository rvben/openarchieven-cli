use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

const SUPPORTED_LANGS: &[&str] = &["nl", "en", "de", "fr"];
const MIN_YEAR: i32 = 1500;
const MAX_YEAR: i32 = 1960;
const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 20;

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub place: Option<String>,
    pub year_start: Option<i32>,
    pub year_end: Option<i32>,
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
            "--offset not supported by `stats professions`",
        ));
    }
    if !SUPPORTED_LANGS.contains(&ctx.lang.as_str()) {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "--lang must be one of {SUPPORTED_LANGS:?}, got {:?}",
                ctx.lang
            ),
        ));
    }
    if let Some(y) = args.year_start
        && !(MIN_YEAR..=MAX_YEAR).contains(&y)
    {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--year-start must be {MIN_YEAR}..={MAX_YEAR}, got {y}"),
        ));
    }
    if let Some(y) = args.year_end
        && !(MIN_YEAR..=MAX_YEAR).contains(&y)
    {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--year-end must be {MIN_YEAR}..={MAX_YEAR}, got {y}"),
        ));
    }
    if let (Some(s), Some(e)) = (args.year_start, args.year_end)
        && s > e
    {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--year-start ({s}) must be <= --year-end ({e})"),
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

    let limit_s = limit.to_string();
    let ys = args.year_start.map(|v| v.to_string());
    let ye = args.year_end.map(|v| v.to_string());

    let mut params: Vec<(&str, &str)> = vec![
        ("number_show", limit_s.as_str()),
        ("lang", ctx.lang.as_str()),
    ];
    if let Some(p) = args.place.as_deref() {
        params.push(("eventplace", p));
    }
    if let Some(s) = ys.as_deref() {
        params.push(("eventyearstart", s));
    }
    if let Some(s) = ye.as_deref() {
        params.push(("eventyearend", s));
    }

    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(24 * 3600)));
    let body = client.get_cached("/stats/professions.json", &params, ttl, cache)?;
    let items = body
        .pointer("/professions")
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
        name: "stats professions",
        description: "Profession frequency stats by place and year range.",
        mutating: false,
        response_shape: "list",
        paginated: false,
        cache_ttl_seconds: Some(24 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "--place",
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
                name: "--year-start",
                ty: "integer",
                required: false,
                positional: false,
                description: None,
                default: None,
                min: Some(MIN_YEAR as i64),
                max: Some(MAX_YEAR as i64),
                r#enum: None,
            },
            Arg {
                name: "--year-end",
                ty: "integer",
                required: false,
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
        assert_eq!(cmd.name, "stats professions");
        let lang_arg = cmd.args.iter().find(|a| a.name == "--lang").unwrap();
        assert!(lang_arg.r#enum.is_some());
        assert_eq!(lang_arg.r#enum.as_ref().unwrap().len(), 4);
    }
}
