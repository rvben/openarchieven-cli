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
    pub birthyear: i32,
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

    let yr = args.birthyear.to_string();
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

    #[test]
    fn schema_returns_correct_shape() {
        let cmd = schema();
        assert_eq!(cmd.name, "match");
        assert!(!cmd.paginated);
        assert_eq!(cmd.response_shape, "list");
        let name_arg = cmd.args.iter().find(|a| a.name == "name").unwrap();
        assert!(name_arg.required);
        assert!(name_arg.positional);
        let birthyear = cmd.args.iter().find(|a| a.name == "birthyear").unwrap();
        assert!(birthyear.required);
        assert!(birthyear.positional);
        let lang = cmd.args.iter().find(|a| a.name == "--lang").unwrap();
        assert!(!lang.required);
        assert!(lang.r#enum.is_some());
        assert_eq!(lang.r#enum.as_ref().unwrap().len(), 2);
        assert_eq!(cmd.output_fields.len(), 3);
        assert!(cmd.output_fields.iter().any(|f| f.name == "items"));
        assert!(cmd.output_fields.iter().any(|f| f.name == "total"));
        assert!(cmd.output_fields.iter().any(|f| f.name == "paginated"));
    }
}
