use chrono::Utc;
use serde_json::json;

use crate::cache::Cache;
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::schema_cmd::{Arg, Command, OutputField};

pub fn info(cache: &Cache) -> Result<Renderable> {
    let snap = cache.info()?;
    Ok(Renderable::single_flat(json!({
        "root": snap.root.display().to_string(),
        "entries": snap.entries,
        "bytes": snap.bytes,
        "oldest": snap.oldest.map(|t| t.to_rfc3339()),
        "newest": snap.newest.map(|t| t.to_rfc3339()),
    })))
}

pub fn clear(cache: &Cache, yes: bool) -> Result<Renderable> {
    if !yes {
        return Err(Error::new(
            ErrorKind::Validation,
            "`cache clear` requires --yes; this is a destructive operation",
        ));
    }
    let deleted = cache.clear()?;
    Ok(Renderable::single_flat(json!({ "deleted": deleted })))
}

pub fn prune(cache: &Cache) -> Result<Renderable> {
    let deleted = cache.prune(Utc::now())?;
    Ok(Renderable::single_flat(json!({ "deleted": deleted })))
}

pub fn info_schema() -> Command {
    Command {
        name: "cache info",
        description: "Show cache location, total size, and entry count",
        mutating: false,
        response_shape: "single-flat",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![],
        output_fields: vec![
            OutputField {
                name: "root",
                ty: "string",
                description: Some("Absolute path to the cache directory"),
            },
            OutputField {
                name: "entries",
                ty: "integer",
                description: Some("Number of cache entry files currently on disk"),
            },
            OutputField {
                name: "bytes",
                ty: "integer",
                description: Some("Total size of all cache entries in bytes"),
            },
            OutputField {
                name: "oldest",
                ty: "datetime | null",
                description: Some(
                    "RFC 3339 timestamp of the oldest cache entry; null if cache is empty",
                ),
            },
            OutputField {
                name: "newest",
                ty: "datetime | null",
                description: Some(
                    "RFC 3339 timestamp of the newest cache entry; null if cache is empty",
                ),
            },
        ],
    }
}

pub fn clear_schema() -> Command {
    Command {
        name: "cache clear",
        description: "Delete all cache entries (requires --yes)",
        mutating: true,
        response_shape: "single-flat",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![Arg {
            name: "--yes",
            ty: "boolean",
            required: true,
            positional: false,
            description: Some("Required safety flag to confirm the destructive clear operation"),
            default: None,
            min: None,
            max: None,
            r#enum: None,
        }],
        output_fields: vec![OutputField {
            name: "deleted",
            ty: "integer",
            description: Some("Number of cache entry files deleted"),
        }],
    }
}

pub fn prune_schema() -> Command {
    Command {
        name: "cache prune",
        description: "Delete only expired cache entries",
        mutating: true,
        response_shape: "single-flat",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![],
        output_fields: vec![OutputField {
            name: "deleted",
            ty: "integer",
            description: Some("Number of expired cache entry files deleted"),
        }],
    }
}
