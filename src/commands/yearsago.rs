use chrono::{Datelike, Local, NaiveDate};

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 10;

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub years: u32,
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
            "--offset is not supported by `yearsago` (limit-only pagination)",
        ));
    }
    if ctx.lang != "nl" {
        return Err(Error::new(
            ErrorKind::Validation,
            "--lang is not supported by `yearsago`",
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

    let target = resolve_yearsago(Local::now().date_naive(), args.years);
    if !ctx.quiet {
        eprintln!(
            "note: searching for records from {:04}-{:02}-{:02}",
            target.year(),
            target.month(),
            target.day()
        );
    }

    let years_s = args.years.to_string();
    let limit_s = limit.to_string();
    let params: Vec<(&str, &str)> = vec![
        ("yearsago", years_s.as_str()),
        ("number_show", limit_s.as_str()),
    ];

    let ttl = resolve_ttl(ctx, TtlHint::UntilMidnight);
    let body = client.get_cached("/records/yearsago.json", &params, ttl, cache)?;

    let items = body
        .pointer("/response/docs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let total = items.len() as u64;

    Ok(
        Renderable::list(serde_json::Value::Array(items), false, None, None)
            .with_total(Some(total)),
    )
}

fn resolve_yearsago(today: NaiveDate, years: u32) -> NaiveDate {
    let target_year = today.year().saturating_sub(years as i32);
    NaiveDate::from_ymd_opt(target_year, today.month(), today.day())
        .or_else(|| NaiveDate::from_ymd_opt(target_year, today.month(), 28))
        .unwrap_or(today)
}

pub fn schema() -> Command {
    Command {
        name: "yearsago",
        description: "Records from N years ago today (date-dependent).",
        mutating: false,
        response_shape: "list",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "until_midnight",
        args: vec![
            Arg {
                name: "years",
                ty: "integer",
                required: true,
                positional: true,
                default: None,
                min: Some(0),
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--limit",
                ty: "integer",
                required: false,
                positional: false,
                default: Some(serde_json::json!(10)),
                min: Some(1),
                max: Some(100),
                r#enum: None,
            },
        ],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<record>",
            },
            OutputField {
                name: "total",
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

    #[test]
    fn resolve_yearsago_subtracts_years() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
        let r = resolve_yearsago(today, 100);
        assert_eq!(r, NaiveDate::from_ymd_opt(1926, 4, 30).unwrap());
    }

    #[test]
    fn resolve_yearsago_zero_returns_today() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
        assert_eq!(resolve_yearsago(today, 0), today);
    }

    #[test]
    fn resolve_yearsago_handles_leap_day() {
        let today = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let r = resolve_yearsago(today, 1);
        // 2023 is not a leap year — fall back to Feb 28
        assert_eq!(r, NaiveDate::from_ymd_opt(2023, 2, 28).unwrap());
    }
}
