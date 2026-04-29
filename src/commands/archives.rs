use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, TtlOverride};
use crate::schema_cmd::{Command, OutputField};

pub fn run(client: &Client, cache: Option<&Cache>, ctx: &ApiContext) -> Result<Renderable> {
    if ctx.limit.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--limit is not supported by `archives` (non-paginated)",
        ));
    }
    if ctx.offset.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--offset is not supported by `archives` (non-paginated)",
        ));
    }

    let ttl = resolve_ttl(ctx, default_ttl());
    let body = client.get_cached("/stats/archives.json", &[], ttl, cache)?;

    let items = body
        .get("archives")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    Ok(Renderable::list(items, false, None, None))
}

fn default_ttl() -> TtlHint {
    TtlHint::Fixed(Duration::from_secs(24 * 3600))
}

fn resolve_ttl(ctx: &ApiContext, default: TtlHint) -> TtlHint {
    match ctx.cache_ttl_override {
        Some(TtlOverride::Disabled) => TtlHint::None,
        Some(TtlOverride::Forever) => TtlHint::Never,
        Some(TtlOverride::Fixed(d)) => TtlHint::Fixed(d),
        None => default,
    }
}

pub fn schema() -> Command {
    Command {
        name: "archives",
        description: "List all participating archives.",
        mutating: false,
        response_shape: "list",
        paginated: false,
        cache_ttl_seconds: Some(24 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<archive>",
            },
            OutputField {
                name: "total",
                ty: "integer",
            },
            OutputField {
                name: "limit",
                ty: "null",
            },
            OutputField {
                name: "offset",
                ty: "null",
            },
            OutputField {
                name: "paginated",
                ty: "boolean",
            },
        ],
    }
}
