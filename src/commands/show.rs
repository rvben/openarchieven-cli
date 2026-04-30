use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command};

const SUPPORTED_LANGS: &[&str] = &["nl", "en"];

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub archive: String,
    pub identifier: String,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    if ctx.fields.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--fields is not supported for `show` (single-nested shape); use `-o json | jq`",
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
    if ctx.limit.is_some() || ctx.offset.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--limit/--offset are not supported by `show`",
        ));
    }

    let params: Vec<(&str, &str)> = vec![
        ("archive_code", args.archive.as_str()),
        ("identifier", args.identifier.as_str()),
        ("lang", ctx.lang.as_str()),
    ];
    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(24 * 3600)));
    let body = client.get_cached("/records/show.json", &params, ttl, cache)?;

    // Empty 2xx body means the API returned "no record" without a 404.
    let is_empty = match &body {
        serde_json::Value::Object(m) => m.is_empty(),
        serde_json::Value::Null => true,
        _ => false,
    };
    if is_empty {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("no record found for {}/{}", args.archive, args.identifier),
        ));
    }

    Ok(Renderable::single_nested(body))
}

pub fn parse_rest(rest: &[String]) -> Result<Args> {
    let mut positional: Vec<String> = Vec::new();
    for tok in rest {
        if tok.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unknown flag {tok} for `show`; no per-command flags supported"),
            ));
        }
        positional.push(tok.clone());
    }
    if positional.len() != 2 {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "show: expected 2 positional args (<archive> <identifier>), got {}",
                positional.len()
            ),
        ));
    }
    Ok(Args {
        archive: positional[0].clone(),
        identifier: positional[1].clone(),
    })
}

pub fn schema() -> Command {
    Command {
        name: "show",
        description: "Fetch a single record by archive code and identifier.",
        mutating: false,
        response_shape: "single-nested",
        paginated: false,
        cache_ttl_seconds: Some(24 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "archive",
                ty: "string",
                required: true,
                positional: true,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "identifier",
                ty: "string",
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
        // Intentional: single-nested shape has no enumerable top-level fields to project.
        output_fields: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_rest_zero_positionals_is_validation_error() {
        let err = parse_rest(&[]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("expected 2"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn parse_rest_one_positional_is_validation_error() {
        let err = parse_rest(&strs(&["a"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("expected 2"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn parse_rest_three_positionals_is_validation_error() {
        let err = parse_rest(&strs(&["a", "b", "c"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("expected 2"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn parse_rest_two_positionals_ok() {
        let a = parse_rest(&strs(&["a", "b"])).unwrap();
        assert_eq!(a.archive, "a");
        assert_eq!(a.identifier, "b");
    }

    #[test]
    fn parse_rest_unknown_flag_is_validation_error() {
        let err = parse_rest(&strs(&["--zzz", "a", "b"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("--zzz"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn parse_rest_flag_mid_positionals_is_validation_error() {
        let err = parse_rest(&strs(&["a", "--flag", "b"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--flag"), "msg: {}", err.message());
    }

    #[test]
    fn schema_returns_correct_shape() {
        let cmd = schema();
        assert_eq!(cmd.name, "show");
        assert_eq!(cmd.response_shape, "single-nested");
        assert!(!cmd.paginated);
        let archive = cmd.args.iter().find(|a| a.name == "archive").unwrap();
        assert!(archive.required);
        assert!(archive.positional);
        let identifier = cmd.args.iter().find(|a| a.name == "identifier").unwrap();
        assert!(identifier.required);
        assert!(identifier.positional);
        let lang = cmd.args.iter().find(|a| a.name == "--lang").unwrap();
        assert!(!lang.required);
        assert!(!lang.positional);
        assert!(lang.r#enum.is_some());
    }
}
