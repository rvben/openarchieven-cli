use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};

#[derive(Debug, Clone, Copy)]
pub struct Endpoint<'a> {
    pub command_name: &'a str,
    pub path: &'a str,
    pub allow_province: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CommonFlags {
    pub event_year: Option<i32>,
    pub event_place: Option<String>,
    pub event_province: Option<String>,
}

const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 10;

pub fn run_event(
    ep: Endpoint<'_>,
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    primary_param: (&str, &str),
    secondary_param: Option<(&str, &str)>,
    flags: &CommonFlags,
) -> Result<Renderable> {
    if ctx.lang != "nl" {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--lang is not supported by `{}`", ep.command_name),
        ));
    }

    if !ep.allow_province && flags.event_province.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--event-province is not supported by `{}`", ep.command_name),
        ));
    }

    let limit = match ctx.limit {
        Some(n) if n > MAX_LIMIT => {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("--limit max is {MAX_LIMIT}, got {n}"),
            ));
        }
        Some(0) => {
            return Err(Error::new(
                ErrorKind::Validation,
                "--limit must be at least 1",
            ));
        }
        Some(n) => n,
        None => DEFAULT_LIMIT,
    };
    let offset = ctx.offset.unwrap_or(0);
    let limit_s = limit.to_string();
    let offset_s = offset.to_string();
    let yr_s = flags.event_year.map(|n| n.to_string());

    let mut params: Vec<(&str, &str)> = vec![
        primary_param,
        ("number_show", limit_s.as_str()),
        ("start", offset_s.as_str()),
    ];
    if let Some(sp) = secondary_param {
        params.push(sp);
    }
    if let Some(s) = yr_s.as_deref() {
        params.push(("eventyear", s));
    }
    if let Some(s) = flags.event_place.as_deref() {
        params.push(("eventplace", s));
    }
    if let Some(s) = flags.event_province.as_deref() {
        params.push(("eventprovince", s));
    }

    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(6 * 3600)));
    let body = client.get_cached(ep.path, &params, ttl, cache)?;

    let total = body.pointer("/response/numFound").and_then(|v| v.as_u64());
    let items = body
        .pointer("/response/docs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(Renderable::list(
        serde_json::Value::Array(items),
        true,
        Some(limit),
        Some(offset),
    )
    .with_total(total))
}

/// Parse common optional flags and collect positional arguments.
///
/// Handles both `--flag VAL` and `--flag=VAL` forms. Rejects unknown flags
/// and `--event-province` when `allow_province` is false.
pub fn parse_common_flags(
    rest: &[String],
    allow_province: bool,
    command_name: &str,
) -> Result<(Vec<String>, CommonFlags)> {
    let mut flags = CommonFlags::default();
    let mut positional = Vec::new();
    let mut iter = rest.iter().peekable();

    while let Some(tok) = iter.next() {
        let s = tok.as_str();

        if let Some(v) = s.strip_prefix("--event-year=") {
            let yr = parse_event_year(v)?;
            flags.event_year = Some(yr);
        } else if s == "--event-year" {
            let v = next_value("--event-year", &mut iter)?;
            flags.event_year = Some(parse_event_year(&v)?);
        } else if let Some(v) = s.strip_prefix("--event-place=") {
            flags.event_place = Some(v.to_string());
        } else if s == "--event-place" {
            let v = next_value("--event-place", &mut iter)?;
            flags.event_place = Some(v);
        } else if let Some(v) = s.strip_prefix("--event-province=") {
            if !allow_province {
                return Err(Error::new(
                    ErrorKind::Validation,
                    format!("--event-province is not supported by `{command_name}`"),
                ));
            }
            flags.event_province = Some(v.to_string());
        } else if s == "--event-province" {
            if !allow_province {
                return Err(Error::new(
                    ErrorKind::Validation,
                    format!("--event-province is not supported by `{command_name}`"),
                ));
            }
            let v = next_value("--event-province", &mut iter)?;
            flags.event_province = Some(v);
        } else if s.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unknown flag {s} for `{command_name}`"),
            ));
        } else {
            positional.push(tok.clone());
        }
    }

    Ok((positional, flags))
}

fn parse_event_year(v: &str) -> Result<i32> {
    v.parse::<i32>().map_err(|_| {
        Error::new(
            ErrorKind::Validation,
            format!("--event-year not an integer: {v}"),
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_common_event_year_eq_form() {
        let (pos, flags) =
            parse_common_flags(&strs(&["jansen", "--event-year=1900"]), true, "births").unwrap();
        assert_eq!(pos, vec!["jansen"]);
        assert_eq!(flags.event_year, Some(1900));
    }

    #[test]
    fn parse_common_event_year_space_form() {
        let (_, flags) =
            parse_common_flags(&strs(&["jansen", "--event-year", "1800"]), true, "births").unwrap();
        assert_eq!(flags.event_year, Some(1800));
    }

    #[test]
    fn parse_common_event_place_space_form() {
        let (_, flags) = parse_common_flags(
            &strs(&["jansen", "--event-place", "Amsterdam"]),
            true,
            "births",
        )
        .unwrap();
        assert_eq!(flags.event_place.as_deref(), Some("Amsterdam"));
    }

    #[test]
    fn parse_common_event_province_eq_form_allowed() {
        let (_, flags) =
            parse_common_flags(&strs(&["jansen", "--event-province=ZH"]), true, "births").unwrap();
        assert_eq!(flags.event_province.as_deref(), Some("ZH"));
    }

    #[test]
    fn parse_common_event_province_space_form_allowed() {
        let (_, flags) =
            parse_common_flags(&strs(&["jansen", "--event-province", "NH"]), true, "births")
                .unwrap();
        assert_eq!(flags.event_province.as_deref(), Some("NH"));
    }

    #[test]
    fn parse_common_event_province_eq_form_disallowed_is_error() {
        let err = parse_common_flags(&strs(&["jansen", "--event-province=ZH"]), false, "deaths")
            .unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
        assert!(err.message().contains("--event-province"));
    }

    #[test]
    fn parse_common_event_province_space_form_disallowed_is_error() {
        let err = parse_common_flags(
            &strs(&["jansen", "--event-province", "ZH"]),
            false,
            "deaths",
        )
        .unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
        assert!(err.message().contains("--event-province"));
    }

    #[test]
    fn parse_common_unknown_flag_is_error() {
        let err = parse_common_flags(&strs(&["jansen", "--unknown"]), true, "births").unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
        assert!(err.message().contains("--unknown"));
    }

    #[test]
    fn parse_common_event_year_not_integer_is_error() {
        let err = parse_common_flags(&strs(&["jansen", "--event-year=notanint"]), true, "births")
            .unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
        assert!(err.message().contains("--event-year"));
    }

    #[test]
    fn parse_common_flag_at_end_is_error() {
        let err =
            parse_common_flags(&strs(&["jansen", "--event-year"]), true, "births").unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
        assert!(err.message().contains("--event-year"));
    }

    #[test]
    fn parse_common_flag_followed_by_flag_is_error() {
        let err = parse_common_flags(
            &strs(&["jansen", "--event-year", "--event-place"]),
            true,
            "births",
        )
        .unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Validation);
    }

    #[test]
    fn parse_common_multiple_positionals_collected() {
        let (pos, _) = parse_common_flags(&strs(&["jan", "anna"]), true, "marriages").unwrap();
        assert_eq!(pos, vec!["jan", "anna"]);
    }
}
