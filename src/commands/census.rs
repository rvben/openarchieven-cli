use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command};

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub year: i32,
    pub place: Option<String>,
    pub gg_uri: Option<String>,
    pub province: Option<String>,
    pub richness: Option<u8>,
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
            "--fields is not supported for `census` (single-nested shape); use `-o json | jq`",
        ));
    }
    if ctx.limit.is_some() || ctx.offset.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--limit/--offset are not supported by `census`",
        ));
    }
    if args.place.is_some() == args.gg_uri.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "exactly one of --place or --gg-uri is required",
        ));
    }
    if let Some(r) = args.richness
        && !(1..=3).contains(&r)
    {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--richness must be 1, 2, or 3 (got {r})"),
        ));
    }

    let year_s = args.year.to_string();
    let richness_s = args.richness.map(|r| r.to_string());
    let mut params: Vec<(&str, &str)> = vec![("year", year_s.as_str())];
    if let Some(p) = args.place.as_deref() {
        params.push(("place", p));
    }
    if let Some(u) = args.gg_uri.as_deref() {
        params.push(("gg_uri", u));
    }
    if let Some(pr) = args.province.as_deref() {
        params.push(("province", pr));
    }
    if let Some(r) = richness_s.as_deref() {
        params.push(("richness", r));
    }

    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(30 * 24 * 3600)));
    let body = client.get_cached("/related/census.json", &params, ttl, cache)?;
    Ok(Renderable::single_nested(body))
}

pub fn parse_rest(rest: &[String]) -> Result<Args> {
    let mut a = Args::default();
    let mut have_year = false;
    let mut iter = rest.iter().peekable();

    while let Some(tok) = iter.next() {
        let s = tok.as_str();
        let (key, val) = if let Some(v) = s.strip_prefix("--year=") {
            ("--year", Some(v.to_string()))
        } else if let Some(v) = s.strip_prefix("--place=") {
            ("--place", Some(v.to_string()))
        } else if let Some(v) = s.strip_prefix("--gg-uri=") {
            ("--gg-uri", Some(v.to_string()))
        } else if let Some(v) = s.strip_prefix("--province=") {
            ("--province", Some(v.to_string()))
        } else if let Some(v) = s.strip_prefix("--richness=") {
            ("--richness", Some(v.to_string()))
        } else if matches!(
            s,
            "--year" | "--place" | "--gg-uri" | "--province" | "--richness"
        ) {
            let v = next_value(s, &mut iter)?;
            (s, Some(v))
        } else if s.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unknown flag {s} for `census`"),
            ));
        } else {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unexpected positional {s} for `census`"),
            ));
        };

        let v = val.unwrap();
        if v.is_empty() {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("{key} requires a non-empty value"),
            ));
        }
        match key {
            "--year" => {
                a.year = v.parse().map_err(|_| {
                    Error::new(
                        ErrorKind::Validation,
                        format!("--year not an integer: {v:?}"),
                    )
                })?;
                have_year = true;
            }
            "--place" => a.place = Some(v),
            "--gg-uri" => a.gg_uri = Some(v),
            "--province" => a.province = Some(v),
            "--richness" => {
                a.richness = Some(v.parse().map_err(|_| {
                    Error::new(
                        ErrorKind::Validation,
                        format!("--richness not an integer: {v:?}"),
                    )
                })?);
            }
            _ => unreachable!(),
        }
    }

    if !have_year {
        return Err(Error::new(
            ErrorKind::Validation,
            "census: --year is required",
        ));
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
        name: "census",
        description: "Census records by place or gg URI.",
        mutating: false,
        response_shape: "single-nested",
        paginated: false,
        cache_ttl_seconds: Some(30 * 24 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "--year",
                ty: "integer",
                required: true,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
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
                name: "--gg-uri",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--province",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--richness",
                ty: "integer",
                required: false,
                positional: false,
                default: None,
                min: Some(1),
                max: Some(3),
                r#enum: Some(vec![
                    serde_json::json!(1),
                    serde_json::json!(2),
                    serde_json::json!(3),
                ]),
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
    fn parse_rest_year_and_place_ok() {
        let a = parse_rest(&strs(&["--year=1850", "--place=Leiden"])).unwrap();
        assert_eq!(a.year, 1850);
        assert_eq!(a.place.as_deref(), Some("Leiden"));
        assert!(a.gg_uri.is_none());
    }

    #[test]
    fn parse_rest_space_form_works() {
        let a = parse_rest(&strs(&["--year", "1850", "--gg-uri", "gg:1"])).unwrap();
        assert_eq!(a.year, 1850);
        assert_eq!(a.gg_uri.as_deref(), Some("gg:1"));
    }

    #[test]
    fn parse_rest_missing_year_is_validation_error() {
        let err = parse_rest(&strs(&["--place=Leiden"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--year"));
    }

    #[test]
    fn parse_rest_unknown_flag_is_validation_error() {
        let err = parse_rest(&strs(&["--year=1850", "--zzz=x"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--zzz"));
    }

    #[test]
    fn parse_rest_unexpected_positional_is_validation_error() {
        let err = parse_rest(&strs(&["--year=1850", "extra"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_rest_richness_parses() {
        let a = parse_rest(&strs(&["--year=1850", "--place=Leiden", "--richness=2"])).unwrap();
        assert_eq!(a.richness, Some(2));
    }

    #[test]
    fn parse_rest_year_not_integer_is_validation_error() {
        let err = parse_rest(&strs(&["--year=abc"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--year"));
    }
}
