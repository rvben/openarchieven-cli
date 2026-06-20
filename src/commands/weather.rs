use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

const SUPPORTED_LANGS: &[&str] = &["nl", "en"];

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub date: String,
    pub longitude: String,
    pub latitude: String,
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
            "--limit/--offset are not supported by `weather`",
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
    validate_iso_date(&args.date)?;
    validate_decimal(&args.longitude, "--longitude")?;
    validate_decimal(&args.latitude, "--latitude")?;

    let params: Vec<(&str, &str)> = vec![
        ("date", args.date.as_str()),
        ("longitude", args.longitude.as_str()),
        ("latitude", args.latitude.as_str()),
        ("lang", ctx.lang.as_str()),
    ];
    let ttl = resolve_ttl(ctx, TtlHint::Never);
    let body = client.get_cached("/related/weather.json", &params, ttl, cache)?;

    let items = match body {
        serde_json::Value::Array(items) => items,
        // Tolerate single-object shape if upstream ever returns a bare object.
        serde_json::Value::Object(_) => vec![body],
        _ => Vec::new(),
    };
    let total = items.len() as u64;
    Ok(
        Renderable::list(serde_json::Value::Array(items), false, None, None)
            .with_total(Some(total)),
    )
}

fn validate_iso_date(s: &str) -> Result<()> {
    let parts: Vec<&str> = s.split('-').collect();
    let ok = parts.len() == 3
        && parts[0].len() == 4
        && parts[0].chars().all(|c| c.is_ascii_digit())
        && parts[1].len() == 2
        && parts[1].chars().all(|c| c.is_ascii_digit())
        && parts[2].len() == 2
        && parts[2].chars().all(|c| c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Validation,
            format!("--date must be YYYY-MM-DD, got {s:?}"),
        ))
    }
}

fn validate_decimal(s: &str, name: &str) -> Result<()> {
    s.parse::<f64>().map(|_| ()).map_err(|_| {
        Error::new(
            ErrorKind::Validation,
            format!("{name} must be a decimal number, got {s:?}"),
        )
    })
}

pub fn schema() -> Command {
    Command {
        name: "weather",
        description: "Historical weather observations for a date and coordinate.",
        mutating: false,
        response_shape: "list",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "never",
        args: vec![
            Arg {
                name: "--date",
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
                name: "--longitude",
                ty: "number",
                required: true,
                positional: false,
                description: None,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--latitude",
                ty: "number",
                required: true,
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
                r#enum: Some(vec![serde_json::json!("nl"), serde_json::json!("en")]),
            },
        ],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<observation>",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_iso_date_accepts_valid() {
        assert!(validate_iso_date("1850-06-15").is_ok());
        assert!(validate_iso_date("2024-12-31").is_ok());
    }

    #[test]
    fn validate_iso_date_rejects_invalid() {
        assert!(validate_iso_date("not-a-date").is_err());
        assert!(validate_iso_date("1850/06/15").is_err());
        assert!(validate_iso_date("18500-06-15").is_err());
        assert!(validate_iso_date("1850-6-15").is_err());
    }

    #[test]
    fn validate_decimal_accepts_valid() {
        assert!(validate_decimal("4.49", "--longitude").is_ok());
        assert!(validate_decimal("-52.16", "--latitude").is_ok());
        assert!(validate_decimal("0", "--longitude").is_ok());
    }

    #[test]
    fn validate_decimal_rejects_non_numeric() {
        let err = validate_decimal("not-a-number", "--longitude").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--longitude"));
    }

    #[test]
    fn validate_iso_date_rejects_non_digit_parts() {
        assert!(validate_iso_date("18ab-06-15").is_err());
        assert!(validate_iso_date("1850-0x-15").is_err());
    }

    #[test]
    fn schema_returns_correct_command_name() {
        let cmd = schema();
        assert_eq!(cmd.name, "weather");
        assert_eq!(cmd.response_shape, "list");
        let date_arg = cmd.args.iter().find(|a| a.name == "--date").unwrap();
        assert!(date_arg.required);
    }
}
