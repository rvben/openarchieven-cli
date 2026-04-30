use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

const SUPPORTED_LANGS: &[&str] = &["nl", "en", "de", "fr"];
const VALID_EVENT_TYPES: &[i32] = &[0, 1, 2, 3, 6];
const MIN_YEAR: i32 = 1500;
const MAX_YEAR: i32 = 1960;
const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 20;

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub place: Option<String>,
    pub year_start: Option<i32>,
    pub year_end: Option<i32>,
    pub event_type: Option<i32>,
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
            "--offset not supported by `stats familynames`",
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
    if let Some(et) = args.event_type
        && !VALID_EVENT_TYPES.contains(&et)
    {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--event-type must be one of {VALID_EVENT_TYPES:?}, got {et}"),
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
    let et = args.event_type.map(|v| v.to_string());

    let mut params: Vec<(&str, &str)> = vec![
        ("number_show", limit_s.as_str()),
        ("lang", ctx.lang.as_str()),
    ];
    if let Some(p) = args.place.as_deref() {
        params.push(("place", p));
    }
    if let Some(s) = ys.as_deref() {
        params.push(("year_start", s));
    }
    if let Some(s) = ye.as_deref() {
        params.push(("year_end", s));
    }
    if let Some(s) = et.as_deref() {
        params.push(("event_type", s));
    }

    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(24 * 3600)));
    let body = client.get_cached("/stats/familynames.json", &params, ttl, cache)?;
    let items = body
        .pointer("/familynames")
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
    let mut a = Args::default();
    let mut iter = rest.iter().peekable();
    const KNOWN: &[&str] = &["--place", "--year-start", "--year-end", "--event-type"];

    while let Some(tok) = iter.next() {
        let s = tok.as_str();
        let (key, val) = if let Some(eq) = s.find('=') {
            let key = &s[..eq];
            if KNOWN.contains(&key) {
                (key.to_string(), s[eq + 1..].to_string())
            } else if s.starts_with("--") {
                return Err(Error::new(
                    ErrorKind::Validation,
                    format!("unknown flag {key} for `stats familynames`"),
                ));
            } else {
                return Err(Error::new(
                    ErrorKind::Validation,
                    format!("unexpected positional {s} for `stats familynames`"),
                ));
            }
        } else if KNOWN.contains(&s) {
            let v = next_value(s, &mut iter)?;
            (s.to_string(), v)
        } else if s.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unknown flag {s} for `stats familynames`"),
            ));
        } else {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unexpected positional {s} for `stats familynames`"),
            ));
        };

        if val.is_empty() {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("{key} requires a non-empty value"),
            ));
        }
        match key.as_str() {
            "--place" => a.place = Some(val),
            "--year-start" => {
                a.year_start = Some(val.parse().map_err(|_| {
                    Error::new(
                        ErrorKind::Validation,
                        format!("--year-start not an integer: {val:?}"),
                    )
                })?);
            }
            "--year-end" => {
                a.year_end = Some(val.parse().map_err(|_| {
                    Error::new(
                        ErrorKind::Validation,
                        format!("--year-end not an integer: {val:?}"),
                    )
                })?);
            }
            "--event-type" => {
                a.event_type = Some(val.parse().map_err(|_| {
                    Error::new(
                        ErrorKind::Validation,
                        format!("--event-type not an integer: {val:?}"),
                    )
                })?);
            }
            _ => unreachable!(),
        }
    }
    Ok(a)
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
        name: "stats familynames",
        description: "Family-name frequency stats by place, year range, and event type.",
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
                default: None,
                min: Some(MIN_YEAR as i64),
                max: Some(MAX_YEAR as i64),
                r#enum: None,
            },
            Arg {
                name: "--event-type",
                ty: "integer",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: Some(vec![
                    serde_json::json!(0),
                    serde_json::json!(1),
                    serde_json::json!(2),
                    serde_json::json!(3),
                    serde_json::json!(6),
                ]),
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
            Arg {
                name: "--lang",
                ty: "string",
                required: false,
                positional: false,
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
    fn parse_no_args_ok() {
        let a = parse_rest(&strs(&[])).unwrap();
        assert!(a.place.is_none() && a.year_start.is_none());
    }

    #[test]
    fn parse_all_flags_eq_form() {
        let a = parse_rest(&strs(&[
            "--place=Leiden",
            "--year-start=1700",
            "--year-end=1800",
            "--event-type=1",
        ]))
        .unwrap();
        assert_eq!(a.place.as_deref(), Some("Leiden"));
        assert_eq!(a.year_start, Some(1700));
        assert_eq!(a.year_end, Some(1800));
        assert_eq!(a.event_type, Some(1));
    }

    #[test]
    fn parse_space_form_works() {
        let a = parse_rest(&strs(&["--place", "Leiden", "--year-start", "1700"])).unwrap();
        assert_eq!(a.place.as_deref(), Some("Leiden"));
        assert_eq!(a.year_start, Some(1700));
    }

    #[test]
    fn parse_unknown_flag_is_validation_error() {
        let err = parse_rest(&strs(&["--zzz=x"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--zzz"));
    }

    #[test]
    fn parse_unexpected_positional_is_validation_error() {
        let err = parse_rest(&strs(&["extra"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_year_not_integer_is_validation_error() {
        let err = parse_rest(&strs(&["--year-start=abc"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--year-start"));
    }

    #[test]
    fn parse_event_type_not_integer_is_validation_error() {
        let err = parse_rest(&strs(&["--event-type=abc"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_empty_value_is_validation_error() {
        let err = parse_rest(&strs(&["--place="])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_year_end_not_integer_is_validation_error() {
        let err = parse_rest(&strs(&["--year-end=abc"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--year-end"));
    }

    #[test]
    fn parse_flag_at_end_is_validation_error() {
        let err = parse_rest(&strs(&["--place"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--place"));
    }

    #[test]
    fn parse_flag_followed_by_flag_is_validation_error() {
        let err = parse_rest(&strs(&["--place", "--year-start"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_unknown_flag_with_eq_is_validation_error() {
        let err = parse_rest(&strs(&["--bogus=x"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--bogus"));
    }

    #[test]
    fn parse_year_end_space_form() {
        let a = parse_rest(&strs(&["--year-end", "1900"])).unwrap();
        assert_eq!(a.year_end, Some(1900));
    }

    #[test]
    fn parse_event_type_space_form() {
        let a = parse_rest(&strs(&["--event-type", "2"])).unwrap();
        assert_eq!(a.event_type, Some(2));
    }

    #[test]
    fn parse_event_type_empty_is_validation_error() {
        let err = parse_rest(&strs(&["--event-type="])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn schema_returns_correct_command_name() {
        let cmd = schema();
        assert_eq!(cmd.name, "stats familynames");
        let et = cmd.args.iter().find(|a| a.name == "--event-type").unwrap();
        assert!(et.r#enum.is_some());
        assert_eq!(et.r#enum.as_ref().unwrap().len(), 5);
    }
}
