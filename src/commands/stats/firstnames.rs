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
        .pointer("/firstnames")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let total = items.len() as u64;
    Ok(
        Renderable::list(serde_json::Value::Array(items), false, Some(limit), None)
            .with_total(Some(total)),
    )
}

pub fn parse_rest(rest: &[String]) -> Result<Args> {
    let mut place: Option<String> = None;
    let mut year: Option<i32> = None;
    let mut iter = rest.iter().peekable();

    while let Some(tok) = iter.next() {
        let s = tok.as_str();
        let (key, val) = if let Some(v) = s.strip_prefix("--place=") {
            ("--place", v.to_string())
        } else if let Some(v) = s.strip_prefix("--year=") {
            ("--year", v.to_string())
        } else if matches!(s, "--place" | "--year") {
            (s, next_value(s, &mut iter)?)
        } else if s.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unknown flag {s} for `stats firstnames`"),
            ));
        } else {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unexpected positional {s} for `stats firstnames`"),
            ));
        };

        if val.is_empty() {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("{key} requires a non-empty value"),
            ));
        }
        match key {
            "--place" => place = Some(val),
            "--year" => {
                year = Some(val.parse().map_err(|_| {
                    Error::new(
                        ErrorKind::Validation,
                        format!("--year not an integer: {val:?}"),
                    )
                })?);
            }
            _ => unreachable!(),
        }
    }

    Ok(Args {
        place: place.ok_or_else(|| {
            Error::new(
                ErrorKind::Validation,
                "stats firstnames: --place is required",
            )
        })?,
        year: year.ok_or_else(|| {
            Error::new(
                ErrorKind::Validation,
                "stats firstnames: --year is required",
            )
        })?,
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
            },
            OutputField {
                name: "total",
                ty: "integer",
            },
            OutputField {
                name: "limit",
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
    fn parse_rest_required_flags_ok() {
        let a = parse_rest(&strs(&["--place=Leiden", "--year=1850"])).unwrap();
        assert_eq!(a.place, "Leiden");
        assert_eq!(a.year, 1850);
    }

    #[test]
    fn parse_rest_space_form_works() {
        let a = parse_rest(&strs(&["--place", "Leiden", "--year", "1850"])).unwrap();
        assert_eq!(a.place, "Leiden");
        assert_eq!(a.year, 1850);
    }

    #[test]
    fn parse_rest_missing_place_is_validation_error() {
        let err = parse_rest(&strs(&["--year=1850"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--place"));
    }

    #[test]
    fn parse_rest_missing_year_is_validation_error() {
        let err = parse_rest(&strs(&["--place=Leiden"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--year"));
    }

    #[test]
    fn parse_rest_unknown_flag_is_validation_error() {
        let err = parse_rest(&strs(&["--zzz=x"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_rest_year_not_integer_is_validation_error() {
        let err = parse_rest(&strs(&["--place=Leiden", "--year=abc"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--year"));
    }

    #[test]
    fn parse_rest_unexpected_positional_is_validation_error() {
        let err = parse_rest(&strs(&["extra"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("extra"));
    }

    #[test]
    fn parse_rest_empty_place_is_validation_error() {
        let err = parse_rest(&strs(&["--place=", "--year=1850"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--place"));
    }

    #[test]
    fn parse_rest_empty_year_is_validation_error() {
        let err = parse_rest(&strs(&["--place=Leiden", "--year="])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--year"));
    }

    #[test]
    fn parse_rest_flag_at_end_is_validation_error() {
        let err = parse_rest(&strs(&["--place"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--place"));
    }

    #[test]
    fn parse_rest_flag_followed_by_flag_is_validation_error() {
        let err = parse_rest(&strs(&["--place", "--year"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

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
