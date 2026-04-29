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
    if ctx.fields.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--fields is not supported for `weather` (single-nested shape); use `-o json | jq`",
        ));
    }
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
    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(30 * 24 * 3600)));
    let body = client.get_cached("/related/weather.json", &params, ttl, cache)?;
    Ok(Renderable::single_nested(body))
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

pub fn parse_rest(rest: &[String]) -> Result<Args> {
    let mut date: Option<String> = None;
    let mut lon: Option<String> = None;
    let mut lat: Option<String> = None;
    let mut iter = rest.iter().peekable();

    while let Some(tok) = iter.next() {
        let s = tok.as_str();
        let (key, val) = if let Some(v) = s.strip_prefix("--date=") {
            ("--date", v.to_string())
        } else if let Some(v) = s.strip_prefix("--longitude=") {
            ("--longitude", v.to_string())
        } else if let Some(v) = s.strip_prefix("--latitude=") {
            ("--latitude", v.to_string())
        } else if matches!(s, "--date" | "--longitude" | "--latitude") {
            (s, next_value(s, &mut iter)?)
        } else if s.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unknown flag {s} for `weather`"),
            ));
        } else {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unexpected positional {s} for `weather`"),
            ));
        };

        if val.is_empty() {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("{key} requires a non-empty value"),
            ));
        }
        match key {
            "--date" => date = Some(val),
            "--longitude" => lon = Some(val),
            "--latitude" => lat = Some(val),
            _ => unreachable!(),
        }
    }

    Ok(Args {
        date: date
            .ok_or_else(|| Error::new(ErrorKind::Validation, "weather: --date is required"))?,
        longitude: lon
            .ok_or_else(|| Error::new(ErrorKind::Validation, "weather: --longitude is required"))?,
        latitude: lat
            .ok_or_else(|| Error::new(ErrorKind::Validation, "weather: --latitude is required"))?,
    })
}

fn next_value(
    flag: &str,
    iter: &mut std::iter::Peekable<std::slice::Iter<'_, String>>,
) -> Result<String> {
    match iter.next() {
        Some(v) if v.starts_with("--") => Err(Error::new(
            ErrorKind::Validation,
            format!("{flag}: missing value (got flag '{v}' instead)"),
        )),
        Some(v) => Ok(v.clone()),
        None => Err(Error::new(
            ErrorKind::Validation,
            format!("{flag}: missing value"),
        )),
    }
}

pub fn schema() -> Command {
    Command {
        name: "weather",
        description: "Historical weather observations for a date and coordinate.",
        mutating: false,
        response_shape: "single-nested",
        paginated: false,
        cache_ttl_seconds: Some(30 * 24 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "--date",
                ty: "string",
                required: true,
                positional: false,
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
    fn parse_rest_all_required_ok() {
        let a = parse_rest(&strs(&[
            "--date=1850-06-15",
            "--longitude=4.49",
            "--latitude=52.16",
        ]))
        .unwrap();
        assert_eq!(a.date, "1850-06-15");
        assert_eq!(a.longitude, "4.49");
        assert_eq!(a.latitude, "52.16");
    }

    #[test]
    fn parse_rest_space_form_works() {
        let a = parse_rest(&strs(&[
            "--date",
            "1850-06-15",
            "--longitude",
            "4.49",
            "--latitude",
            "52.16",
        ]))
        .unwrap();
        assert_eq!(a.date, "1850-06-15");
    }

    #[test]
    fn parse_rest_missing_date_is_validation_error() {
        let err = parse_rest(&strs(&["--longitude=4.49", "--latitude=52.16"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--date"));
    }

    #[test]
    fn parse_rest_missing_longitude_is_validation_error() {
        let err = parse_rest(&strs(&["--date=1850-06-15", "--latitude=52.16"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--longitude"));
    }

    #[test]
    fn parse_rest_unknown_flag_is_validation_error() {
        let err = parse_rest(&strs(&["--zzz=x"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--zzz"));
    }

    #[test]
    fn parse_rest_unexpected_positional_is_validation_error() {
        let err = parse_rest(&strs(&[
            "extra",
            "--date=1850-06-15",
            "--longitude=4.49",
            "--latitude=52.16",
        ]))
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

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
}
