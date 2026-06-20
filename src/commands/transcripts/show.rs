use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command};

pub const SUPPORTED_LANGS: &[&str] = &["nl", "en", "de", "fr"];

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub id: String,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    if args.id.is_empty() {
        return Err(Error::new(ErrorKind::Validation, "id: must not be empty"));
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
    if ctx.limit.is_some() || ctx.offset.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--limit/--offset are not supported by `transcripts show`",
        ));
    }

    let params: Vec<(&str, &str)> = vec![("id", args.id.as_str()), ("lang", ctx.lang.as_str())];
    let ttl = resolve_ttl(ctx, TtlHint::Never);
    let body = client.get_cached("/transcriptions/show.json", &params, ttl, cache)?;

    if body.get("error_code").is_some()
        && let Some(desc) = body.get("error_description").and_then(|v| v.as_str())
    {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "no transcription found for {} (upstream: {})",
                args.id, desc
            ),
        ));
    }

    let is_empty = match &body {
        serde_json::Value::Object(m) => m.is_empty(),
        serde_json::Value::Null => true,
        _ => false,
    };
    if is_empty {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("no transcription found for {}", args.id),
        ));
    }

    Ok(Renderable::single_nested(body))
}

pub fn schema() -> Command {
    Command {
        name: "transcripts show",
        description: "Retrieve the full transcript of a single page by id (immutable; cached forever).",
        mutating: false,
        response_shape: "single-nested",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "never",
        args: vec![
            Arg {
                name: "id",
                ty: "string",
                required: true,
                positional: true,
                description: Some("Unique transcription page identifier"),
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
                r#enum: Some(vec![
                    serde_json::json!("nl"),
                    serde_json::json!("en"),
                    serde_json::json!("de"),
                    serde_json::json!("fr"),
                ]),
            },
        ],
        // Single-nested shape has no enumerable top-level fields to project.
        output_fields: vec![],
    }
}
