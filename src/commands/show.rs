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
        ("archive", args.archive.as_str()),
        ("identifier", args.identifier.as_str()),
        ("lang", ctx.lang.as_str()),
    ];
    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(24 * 3600)));
    let body = client.get_cached("/records/show.json", &params, ttl, cache)?;

    // The upstream API returns HTTP 200 with an error envelope instead of a 4xx.
    // Translate it to NotFound so the stderr+exit-1 contract holds.
    if body.get("error_code").is_some()
        && let Some(desc) = body.get("error_description").and_then(|v| v.as_str())
    {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "no record found for {}/{} (upstream: {})",
                args.archive, args.identifier, desc
            ),
        ));
    }

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
                description: Some("Archive code identifying the holding institution"),
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
                description: Some("Record identifier within the archive"),
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
                description: Some("Response language for labels and descriptions"),
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
