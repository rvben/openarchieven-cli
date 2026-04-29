use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

const SUPPORTED_LANGS: &[&str] = &["nl", "en"];

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub name: String,
    pub birth_year: i32,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    if ctx.limit.is_some() || ctx.offset.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--limit/--offset are not supported by `match`",
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

    let yr = args.birth_year.to_string();
    let params: Vec<(&str, &str)> = vec![
        ("name", args.name.as_str()),
        ("birth_year", yr.as_str()),
        ("lang", ctx.lang.as_str()),
    ];
    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(6 * 3600)));
    let body = client.get_cached("/records/match.json", &params, ttl, cache)?;

    let total = body.pointer("/response/numFound").and_then(|v| v.as_u64());
    let items = body
        .pointer("/response/docs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(Renderable::list(serde_json::Value::Array(items), false, None, None).with_total(total))
}

pub fn parse_rest(rest: &[String]) -> Result<Args> {
    let mut positional: Vec<String> = Vec::new();
    for tok in rest {
        if tok.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unknown flag {tok} for `match`; no per-command flags supported"),
            ));
        }
        positional.push(tok.clone());
    }
    if positional.len() != 2 {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "match: expected 2 positional args (<name> <birthyear>), got {}",
                positional.len()
            ),
        ));
    }
    let birth_year: i32 = positional[1].parse().map_err(|_| {
        Error::new(
            ErrorKind::Validation,
            format!(
                "match: birthyear must be an integer, got {:?}",
                positional[1]
            ),
        )
    })?;
    Ok(Args {
        name: positional[0].clone(),
        birth_year,
    })
}

pub fn schema() -> Command {
    Command {
        name: "match",
        description: "Match a record by name and birth year (probabilistic linkage).",
        mutating: false,
        response_shape: "list",
        paginated: false,
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
                name: "birthyear",
                ty: "integer",
                required: true,
                positional: true,
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
    fn parse_rest_two_positionals_ok() {
        let a = parse_rest(&strs(&["jansen", "1850"])).unwrap();
        assert_eq!(a.name, "jansen");
        assert_eq!(a.birth_year, 1850);
    }

    #[test]
    fn parse_rest_one_positional_is_validation_error() {
        let err = parse_rest(&strs(&["jansen"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_rest_non_integer_birthyear_is_validation_error() {
        let err = parse_rest(&strs(&["jansen", "abc"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("birthyear"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn parse_rest_unknown_flag_is_validation_error() {
        let err = parse_rest(&strs(&["--zzz", "jansen", "1850"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }
}
