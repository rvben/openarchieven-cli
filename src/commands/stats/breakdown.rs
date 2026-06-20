use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

pub const SUPPORTED_LANGS: &[&str] = &["nl", "en", "de", "fr"];
pub const GROUP_BY_VALUES: &[&str] = &["archive", "sourcetype", "eventtype", "place", "year"];
pub const SORT_VALUES: &[&str] = &["count_desc", "count_asc", "name_asc"];
pub const EVENT_TYPE_VALUES: &[i32] = &[0, 1, 2, 3, 6];
const MIN_YEAR: i32 = 1500;
const MAX_YEAR: i32 = 1960;
const MAX_LIMIT: u32 = 500;
const DEFAULT_LIMIT: u32 = 100;

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub group_by: String,
    pub archive: Option<String>,
    pub source_type: Option<String>,
    pub event_type: Option<i32>,
    pub place: Option<String>,
    pub year_start: Option<i32>,
    pub year_end: Option<i32>,
    pub min_count: Option<u32>,
    pub sort: Option<String>,
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
            "--offset not supported by `stats breakdown`",
        ));
    }
    if !GROUP_BY_VALUES.contains(&args.group_by.as_str()) {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "group_by must be one of {GROUP_BY_VALUES:?}, got {:?}",
                args.group_by
            ),
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
    if let Some(et) = args.event_type
        && !EVENT_TYPE_VALUES.contains(&et)
    {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--event-type must be one of {EVENT_TYPE_VALUES:?}, got {et}"),
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
    if let Some(0) = args.min_count {
        return Err(Error::new(
            ErrorKind::Validation,
            "--min-count must be >= 1",
        ));
    }
    if let Some(ref s) = args.sort
        && !SORT_VALUES.contains(&s.as_str())
    {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--sort must be one of {SORT_VALUES:?}, got {s:?}"),
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
    let mc = args.min_count.map(|v| v.to_string());

    let mut params: Vec<(&str, &str)> = vec![
        ("group_by", args.group_by.as_str()),
        ("number_show", limit_s.as_str()),
        ("lang", ctx.lang.as_str()),
    ];
    if let Some(v) = args.archive.as_deref() {
        params.push(("archive_code", v));
    }
    if let Some(v) = args.source_type.as_deref() {
        params.push(("sourcetype", v));
    }
    if let Some(s) = et.as_deref() {
        params.push(("eventtype", s));
    }
    if let Some(v) = args.place.as_deref() {
        params.push(("eventplace", v));
    }
    if let Some(s) = ys.as_deref() {
        params.push(("year_start", s));
    }
    if let Some(s) = ye.as_deref() {
        params.push(("year_end", s));
    }
    if let Some(s) = mc.as_deref() {
        params.push(("min_count", s));
    }
    if let Some(s) = args.sort.as_deref() {
        params.push(("sort", s));
    }

    let ttl = resolve_ttl(ctx, TtlHint::Fixed(Duration::from_secs(24 * 3600)));
    let body = client.get_cached("/stats/breakdown.json", &params, ttl, cache)?;
    Ok(Renderable::single_nested(body))
}

pub fn schema() -> Command {
    Command {
        name: "stats breakdown",
        description: "Cross-tabulation aggregation grouped by one dimension.",
        mutating: false,
        response_shape: "single-nested",
        paginated: false,
        cache_ttl_seconds: Some(24 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "group_by",
                ty: "string",
                required: true,
                positional: true,
                description: Some(
                    "Dimension to group aggregation by: archive, sourcetype, eventtype, place, or year",
                ),
                default: None,
                min: None,
                max: None,
                r#enum: Some(
                    GROUP_BY_VALUES
                        .iter()
                        .map(|s| serde_json::json!(s))
                        .collect(),
                ),
            },
            Arg {
                name: "--archive",
                ty: "string",
                required: false,
                positional: false,
                description: Some("Filter results to a specific archive code"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--source-type",
                ty: "string",
                required: false,
                positional: false,
                description: Some("Filter results to a specific source type"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--event-type",
                ty: "integer",
                required: false,
                positional: false,
                description: Some(
                    "Filter by event type: 0=all, 1=birth, 2=marriage, 3=death, 6=notarial",
                ),
                default: None,
                min: None,
                max: None,
                r#enum: Some(vec![
                    serde_json::json!({"value": 0, "label": "all", "description": "All event types"}),
                    serde_json::json!({"value": 1, "label": "birth", "description": "Geboorte"}),
                    serde_json::json!({"value": 2, "label": "marriage", "description": "Huwelijk"}),
                    serde_json::json!({"value": 3, "label": "death", "description": "Overlijden"}),
                    serde_json::json!({"value": 6, "label": "notarial", "description": "Notariële akten"}),
                ]),
            },
            Arg {
                name: "--place",
                ty: "string",
                required: false,
                positional: false,
                description: Some("Filter results by event place name"),
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
                description: Some("Start of event year range (inclusive, 1500-1960)"),
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
                description: Some("End of event year range (inclusive, 1500-1960)"),
                default: None,
                min: Some(MIN_YEAR as i64),
                max: Some(MAX_YEAR as i64),
                r#enum: None,
            },
            Arg {
                name: "--min-count",
                ty: "integer",
                required: false,
                positional: false,
                description: Some("Exclude groups with fewer than this many records"),
                default: None,
                min: Some(1),
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--sort",
                ty: "string",
                required: false,
                positional: false,
                description: Some("Sort order for results: count_desc, count_asc, or name_asc"),
                default: Some(serde_json::json!("count_desc")),
                min: None,
                max: None,
                r#enum: Some(SORT_VALUES.iter().map(|s| serde_json::json!(s)).collect()),
            },
            Arg {
                name: "--limit",
                ty: "integer",
                required: false,
                positional: false,
                description: Some("Maximum number of groups to return (1-500)"),
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
                description: Some("Response language for labels and descriptions"),
                default: Some(serde_json::json!("nl")),
                min: None,
                max: None,
                r#enum: Some(
                    SUPPORTED_LANGS
                        .iter()
                        .map(|s| serde_json::json!(s))
                        .collect(),
                ),
            },
        ],
        output_fields: vec![
            OutputField {
                name: "group_by",
                ty: "string",
                description: Some("Dimension used for grouping in this response"),
            },
            OutputField {
                name: "filters",
                ty: "object",
                description: Some("Active filter values applied to this aggregation"),
            },
            OutputField {
                name: "total_records",
                ty: "integer",
                description: Some("Total number of records across all groups"),
            },
            OutputField {
                name: "total_groups",
                ty: "integer",
                description: Some("Total number of distinct groups found"),
            },
            OutputField {
                name: "results",
                ty: "array<group>",
                description: Some("Aggregated group objects, each with a label and count"),
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
        assert_eq!(cmd.name, "stats breakdown");
        assert_eq!(cmd.response_shape, "single-nested");
        let gb = cmd.args.iter().find(|a| a.name == "group_by").unwrap();
        assert!(gb.required);
        assert!(gb.positional);
        assert_eq!(gb.r#enum.as_ref().unwrap().len(), GROUP_BY_VALUES.len());
        let sort = cmd.args.iter().find(|a| a.name == "--sort").unwrap();
        assert_eq!(sort.r#enum.as_ref().unwrap().len(), SORT_VALUES.len());
    }
}
