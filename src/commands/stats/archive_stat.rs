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

pub fn parse_archive_rest(rest: &[String], command_name: &str) -> Result<ArchiveStatArgs> {
    let mut a = ArchiveStatArgs::default();
    let mut iter = rest.iter().peekable();
    while let Some(tok) = iter.next() {
        let s = tok.as_str();
        if let Some(v) = s.strip_prefix("--archive=") {
            if v.is_empty() {
                return Err(Error::new(
                    ErrorKind::Validation,
                    "--archive requires a non-empty value",
                ));
            }
            a.archive = Some(v.to_string());
        } else if s == "--archive" {
            let v = iter
                .next()
                .ok_or_else(|| Error::new(ErrorKind::Validation, "--archive: missing value"))?;
            if v.starts_with("--") {
                return Err(Error::new(
                    ErrorKind::Validation,
                    format!("--archive: missing value (got flag '{v}' instead)"),
                ));
            }
            a.archive = Some(v.clone());
        } else if s.starts_with("--") {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unknown flag {s} for `stats {command_name}`"),
            ));
        } else {
            return Err(Error::new(
                ErrorKind::Validation,
                format!("unexpected positional {s} for `stats {command_name}`"),
            ));
        }
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_no_args_ok() {
        let a = parse_archive_rest(&strs(&[]), "records").unwrap();
        assert!(a.archive.is_none());
    }

    #[test]
    fn parse_archive_eq_form() {
        let a = parse_archive_rest(&strs(&["--archive=elo"]), "records").unwrap();
        assert_eq!(a.archive.as_deref(), Some("elo"));
    }

    #[test]
    fn parse_archive_space_form() {
        let a = parse_archive_rest(&strs(&["--archive", "elo"]), "records").unwrap();
        assert_eq!(a.archive.as_deref(), Some("elo"));
    }

    #[test]
    fn parse_unknown_flag_is_validation_error() {
        let err = parse_archive_rest(&strs(&["--zzz=x"]), "records").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--zzz"));
    }

    #[test]
    fn parse_unexpected_positional_is_validation_error() {
        let err = parse_archive_rest(&strs(&["extra"]), "records").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_archive_empty_value_rejected() {
        let err = parse_archive_rest(&strs(&["--archive="]), "records").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn parse_archive_missing_value_rejected() {
        let err = parse_archive_rest(&strs(&["--archive"]), "records").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--archive"));
    }

    #[test]
    fn parse_archive_followed_by_flag_rejected() {
        let err = parse_archive_rest(&strs(&["--archive", "--zzz"]), "records").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }
}
