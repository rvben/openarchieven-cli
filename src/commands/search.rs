use std::time::Duration;

use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command, OutputField};

pub const SUPPORTED_LANGS: &[&str] = &["nl", "en"];
pub const MAX_LIMIT: u32 = 100;
pub const DEFAULT_LIMIT: u32 = 10;

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub name: String,
    pub archive: Option<String>,
    pub source_type: Option<String>,
    pub event_place: Option<String>,
    pub birth_place: Option<String>,
    pub relation_type: Option<String>,
    pub country: Option<String>,
    pub sort: Option<i32>,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    if !SUPPORTED_LANGS.contains(&ctx.lang.as_str()) {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "--lang: unsupported language '{}', supported: {}",
                ctx.lang,
                SUPPORTED_LANGS.join(", ")
            ),
        ));
    }

    let limit = ctx.limit.unwrap_or(DEFAULT_LIMIT);
    if limit > MAX_LIMIT {
        return Err(Error::new(
            ErrorKind::Validation,
            format!("--limit: exceeds maximum of {MAX_LIMIT}"),
        ));
    }
    if limit == 0 {
        return Err(Error::new(
            ErrorKind::Validation,
            "--limit: must be at least 1",
        ));
    }

    let offset = ctx.offset.unwrap_or(0);

    let limit_str = limit.to_string();
    let start_str = offset.to_string();
    let mut params: Vec<(&str, &str)> = vec![
        ("name", args.name.as_str()),
        ("number_show", &limit_str),
        ("start", &start_str),
        ("lang", ctx.lang.as_str()),
    ];

    if let Some(ref v) = args.archive {
        params.push(("archive_code", v.as_str()));
    }
    if let Some(ref v) = args.source_type {
        params.push(("sourcetype", v.as_str()));
    }
    if let Some(ref v) = args.event_place {
        params.push(("eventplace", v.as_str()));
    }
    if let Some(ref v) = args.birth_place {
        params.push(("birthplace", v.as_str()));
    }
    if let Some(ref v) = args.relation_type {
        params.push(("relationtype", v.as_str()));
    }
    if let Some(ref v) = args.country {
        params.push(("country_code", v.as_str()));
    }
    let sort_str;
    if let Some(s) = args.sort {
        sort_str = s.to_string();
        params.push(("sort", &sort_str));
    }

    let ttl = resolve_ttl(ctx, default_ttl());
    let body = client.get_cached("/records/search.json", &params, ttl, cache)?;

    let items = body
        .pointer("/response/docs")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    let total = body.pointer("/response/numFound").and_then(|v| v.as_u64());

    Ok(Renderable::list(items, true, Some(limit), Some(offset)).with_total(total))
}

pub fn default_ttl() -> TtlHint {
    TtlHint::Fixed(Duration::from_secs(6 * 3600))
}

pub fn schema() -> Command {
    Command {
        name: "search",
        description: "Free-text record search across all archives.",
        mutating: false,
        response_shape: "list",
        paginated: true,
        cache_ttl_seconds: Some(6 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "name",
                ty: "string",
                required: true,
                positional: true,
                description: Some("Name to search for across all archives"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--archive",
                ty: "string",
                required: false,
                positional: false,
                description: Some("Filter by archive code"),
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
                description: Some("Filter by source type"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--event-place",
                ty: "string",
                required: false,
                positional: false,
                description: Some("Filter by place where the event occurred"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--birth-place",
                ty: "string",
                required: false,
                positional: false,
                description: Some("Filter by place of birth"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--relation-type",
                ty: "string",
                required: false,
                positional: false,
                description: Some("Filter by relation type of the person to the record"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--country",
                ty: "string",
                required: false,
                positional: false,
                description: Some("Filter by country code"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--sort",
                ty: "integer",
                required: false,
                positional: false,
                description: Some(
                    "Sort order integer (see enum values); default 1 = name ascending",
                ),
                default: Some(serde_json::json!(1)),
                min: None,
                max: None,
                r#enum: Some(vec![
                    serde_json::json!({"value": -6, "label": "source_desc", "description": "Sort by source descending"}),
                    serde_json::json!({"value": -5, "label": "place_desc", "description": "Sort by place descending"}),
                    serde_json::json!({"value": -4, "label": "date_desc", "description": "Sort by date descending"}),
                    serde_json::json!({"value": -3, "label": "event_desc", "description": "Sort by event type descending"}),
                    serde_json::json!({"value": -2, "label": "role_desc", "description": "Sort by role descending"}),
                    serde_json::json!({"value": -1, "label": "name_desc", "description": "Sort by name descending"}),
                    serde_json::json!({"value": 1, "label": "name_asc", "description": "Sort by name ascending (default)"}),
                    serde_json::json!({"value": 2, "label": "role_asc", "description": "Sort by role ascending"}),
                    serde_json::json!({"value": 3, "label": "event_asc", "description": "Sort by event type ascending"}),
                    serde_json::json!({"value": 4, "label": "date_asc", "description": "Sort by date ascending"}),
                    serde_json::json!({"value": 5, "label": "place_asc", "description": "Sort by place ascending"}),
                    serde_json::json!({"value": 6, "label": "source_asc", "description": "Sort by source ascending"}),
                ]),
            },
            Arg {
                name: "--limit",
                ty: "integer",
                required: false,
                positional: false,
                description: Some("Maximum number of results per page (1-100)"),
                default: Some(serde_json::json!(10)),
                min: None,
                max: Some(100),
                r#enum: None,
            },
            Arg {
                name: "--offset",
                ty: "integer",
                required: false,
                positional: false,
                description: Some("Zero-based index of the first result to return"),
                default: Some(serde_json::json!(0)),
                min: None,
                max: None,
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
                r#enum: Some(vec![serde_json::json!("nl"), serde_json::json!("en")]),
            },
        ],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<record>",
                description: Some("Matched record objects from /response/docs"),
            },
            OutputField {
                name: "total",
                ty: "integer | null",
                description: Some("Total hit count from /response/numFound; null if unavailable"),
            },
            OutputField {
                name: "limit",
                ty: "integer",
                description: Some("Maximum number of items returned in this page"),
            },
            OutputField {
                name: "offset",
                ty: "integer",
                description: Some("Zero-based index of the first item in this page"),
            },
            OutputField {
                name: "paginated",
                ty: "boolean",
                description: Some("Always true; this command supports pagination"),
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
        assert_eq!(cmd.name, "search");
        assert!(cmd.paginated);
        assert_eq!(cmd.response_shape, "list");
        let sort_arg = cmd.args.iter().find(|a| a.name == "--sort").unwrap();
        assert!(sort_arg.r#enum.is_some());
        assert_eq!(sort_arg.r#enum.as_ref().unwrap().len(), 12);
        let name_arg = cmd.args.iter().find(|a| a.name == "name").unwrap();
        assert!(name_arg.required);
        assert!(name_arg.positional);
        let limit_arg = cmd.args.iter().find(|a| a.name == "--limit").unwrap();
        assert_eq!(limit_arg.max, Some(100));
        let lang_arg = cmd.args.iter().find(|a| a.name == "--lang").unwrap();
        assert!(lang_arg.r#enum.is_some());
        let archive_arg = cmd.args.iter().find(|a| a.name == "--archive").unwrap();
        assert!(!archive_arg.required);
        let source_type = cmd.args.iter().find(|a| a.name == "--source-type").unwrap();
        assert!(!source_type.required);
        let event_place = cmd.args.iter().find(|a| a.name == "--event-place").unwrap();
        assert!(!event_place.required);
        let birth_place = cmd.args.iter().find(|a| a.name == "--birth-place").unwrap();
        assert!(!birth_place.required);
        let relation_type = cmd
            .args
            .iter()
            .find(|a| a.name == "--relation-type")
            .unwrap();
        assert!(!relation_type.required);
        let country = cmd.args.iter().find(|a| a.name == "--country").unwrap();
        assert!(!country.required);
        assert_eq!(cmd.output_fields.len(), 5);
        assert!(cmd.output_fields.iter().any(|f| f.name == "items"));
        assert!(cmd.output_fields.iter().any(|f| f.name == "total"));
    }
}
