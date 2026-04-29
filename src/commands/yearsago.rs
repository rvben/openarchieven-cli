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

pub fn parse_rest(rest: &[String]) -> Result<Args> {
    let mut positional: Vec<String> = Vec::new();
    for tok in rest {
        if tok.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unknown flag {tok} for `yearsago`; no per-command flags supported"),
            ));
        }
        positional.push(tok.clone());
    }
    if positional.len() != 1 {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "yearsago: expected 1 positional arg (<years>), got {}",
                positional.len()
            ),
        ));
    }
    let years: u32 = positional[0].parse().map_err(|_| {
        Error::new(
            ErrorKind::Validation,
            format!(
                "yearsago: years must be a non-negative integer, got {:?}",
                positional[0]
            ),
        )
    })?;
    Ok(Args { years })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_rest_one_positional_ok() {
        let a = parse_rest(&strs(&["100"])).unwrap();
        assert_eq!(a.years, 100);
    }

    #[test]
    fn parse_rest_zero_positionals_is_validation_error() {
        let err = parse_rest(&[]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("expected 1"));
    }

    #[test]
    fn parse_rest_two_positionals_is_validation_error() {
        let err = parse_rest(&strs(&["100", "200"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_rest_non_integer_years_is_validation_error() {
        let err = parse_rest(&strs(&["abc"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("years"));
    }

    #[test]
    fn parse_rest_negative_years_is_validation_error() {
        let err = parse_rest(&strs(&["-5"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_rest_unknown_flag_is_validation_error() {
        let err = parse_rest(&strs(&["--zzz", "100"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--zzz"));
    }
}
