use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

const SUPPORTED_LANGS: &[&str] = &["nl", "en", "de", "fr"];
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
    let items = decode_familynames_response(&body);
    let total = items.len() as u64;
    Ok(
        Renderable::list(serde_json::Value::Array(items), false, Some(limit), None)
            .with_total(Some(total)),
    )
}

/// Decode the upstream `/stats/familynames.json` response.
///
/// Modern responses use a Google-Charts-style `{cols:[{id,label,type}], rows:[{c:[{v}]}]}`
/// payload; flatten into a list of records keyed by `cols[i].id`. Cells with no
/// `v` become `null`. Falls back to the legacy `{"familynames": [...]}` wrapped
/// shape if encountered.
fn decode_familynames_response(body: &serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(legacy) = body.pointer("/familynames").and_then(|v| v.as_array()) {
        return legacy.clone();
    }
    let Some(cols) = body.pointer("/cols").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let Some(rows) = body.pointer("/rows").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let keys: Vec<&str> = cols
        .iter()
        .map(|c| c.get("id").and_then(|v| v.as_str()).unwrap_or(""))
        .collect();
    rows.iter()
        .map(|row| {
            let cells = row.pointer("/c").and_then(|v| v.as_array());
            let mut obj = serde_json::Map::new();
            if let Some(cells) = cells {
                for (i, cell) in cells.iter().enumerate() {
                    let key = keys.get(i).copied().unwrap_or("");
                    if key.is_empty() {
                        continue;
                    }
                    let val = cell.get("v").cloned().unwrap_or(serde_json::Value::Null);
                    obj.insert(key.to_string(), val);
                }
            }
            serde_json::Value::Object(obj)
        })
        .collect()
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
                    serde_json::json!({"value": 0, "label": "all", "description": "All event types"}),
                    serde_json::json!({"value": 1, "label": "birth", "description": "Geboorte"}),
                    serde_json::json!({"value": 2, "label": "death", "description": "Overlijden"}),
                    serde_json::json!({"value": 3, "label": "marriage", "description": "Huwelijk"}),
                    serde_json::json!({"value": 6, "label": "other", "description": "Other registrations"}),
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

    #[test]
    fn schema_returns_correct_command_name() {
        let cmd = schema();
        assert_eq!(cmd.name, "stats familynames");
        let et = cmd.args.iter().find(|a| a.name == "--event-type").unwrap();
        assert!(et.r#enum.is_some());
        assert_eq!(et.r#enum.as_ref().unwrap().len(), 5);
    }

    #[test]
    fn decode_familynames_response_basic() {
        let body = serde_json::json!({
            "cols": [{"id":"name"},{"id":"count"}],
            "rows": [
                {"c":[{"v":"Jansen"},{"v":42}]},
                {"c":[{"v":"Vries"},{"v":31}]},
            ]
        });
        let out = decode_familynames_response(&body);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["name"], "Jansen");
        assert_eq!(out[0]["count"], 42);
        assert_eq!(out[1]["name"], "Vries");
        assert_eq!(out[1]["count"], 31);
    }

    #[test]
    fn decode_familynames_response_legacy_wrapped() {
        let body = serde_json::json!({"familynames": [{"name":"X"}]});
        let out = decode_familynames_response(&body);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["name"], "X");
    }

    #[test]
    fn decode_familynames_response_empty_input() {
        let body = serde_json::json!({"cols":[],"rows":[]});
        assert!(decode_familynames_response(&body).is_empty());
    }

    #[test]
    fn decode_familynames_response_missing_v_becomes_null() {
        let body = serde_json::json!({
            "cols": [{"id":"name"},{"id":"count"}],
            "rows": [{"c":[{"v":"Jansen"},{}]}]
        });
        let out = decode_familynames_response(&body);
        assert_eq!(out[0]["name"], "Jansen");
        assert_eq!(out[0]["count"], serde_json::Value::Null);
    }
}
