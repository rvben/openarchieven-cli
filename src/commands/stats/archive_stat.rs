use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};

#[derive(Debug, Clone, Default)]
pub struct ArchiveStatArgs {
    pub archive: Option<String>,
}

/// `items_pointer` is the legacy wrapped-shape pointer (e.g. `/records`).
/// Upstream now returns a bare array; the wrapped form is accepted as a fallback.
pub fn run_archive_stat(
    command_name: &str,
    path: &str,
    items_pointer: &str,
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &ArchiveStatArgs,
) -> Result<Renderable> {
    if ctx.limit.is_some() || ctx.offset.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--limit/--offset not supported by `stats {command_name}`"),
        ));
    }
    if ctx.lang != "nl" {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--lang not supported by `stats {command_name}`"),
        ));
    }

    let mut params: Vec<(&str, &str)> = vec![];
    if let Some(a) = args.archive.as_deref() {
        params.push(("archive_code", a));
    }

    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(24 * 3600)));
    let body = client.get_cached(path, &params, ttl, cache)?;
    let items = match body {
        serde_json::Value::Array(arr) => arr,
        other => other
            .pointer(items_pointer)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
    };
    let total = items.len() as u64;
    Ok(
        Renderable::list(serde_json::Value::Array(items), false, None, None)
            .with_total(Some(total)),
    )
}
