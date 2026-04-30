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

    // The upstream API accepts `eventyear=` but does not reliably filter on it.
    // Post-filter client-side to enforce the advertised behaviour. Docs that
    // lack `eventdate.year` entirely are kept rather than silently dropped,
    // because an absent year field should not cause valid records to vanish.
    let items: Vec<serde_json::Value> = if let Some(yr) = flags.event_year {
        items
            .into_iter()
            .filter(|d| {
                d.pointer("/eventdate/year")
                    .and_then(|v| v.as_i64())
                    .map(|y| y as i32 == yr)
                    .unwrap_or(true)
            })
            .collect()
    } else {
        items
    };

    Ok(Renderable::list(
        serde_json::Value::Array(items),
        true,
        Some(limit),
        Some(offset),
    )
    .with_total(total))
}
