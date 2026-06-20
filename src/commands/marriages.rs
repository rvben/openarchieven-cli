use crate::cache::Cache;
use crate::client::Client;
use crate::commands::event_records::{CommonFlags, Endpoint, run_event};
use crate::output::Renderable;
use crate::runtime::ApiContext;
use crate::schema_cmd::{Arg, Command, OutputField};

#[derive(Debug)]
pub struct Args {
    pub name1: String,
    pub name2: String,
    pub flags: CommonFlags,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable, crate::error::Error> {
    run_event(
        Endpoint {
            command_name: "marriages",
            path: "/records/getMarriages.json",
            allow_province: false,
        },
        client,
        cache,
        ctx,
        ("name1", args.name1.as_str()),
        Some(("name2", args.name2.as_str())),
        &args.flags,
    )
}

pub fn schema() -> Command {
    Command {
        name: "marriages",
        description: "Marriage-event records by both partners' names.",
        mutating: false,
        response_shape: "list",
        paginated: true,
        cache_ttl_seconds: Some(6 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "name1",
                ty: "string",
                required: true,
                positional: true,
                description: Some("Name of the first partner"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "name2",
                ty: "string",
                required: true,
                positional: true,
                description: Some("Name of the second partner"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--event-year",
                ty: "integer",
                required: false,
                positional: false,
                description: Some("Filter by year of marriage event; client-side post-filtered"),
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
                description: Some("Filter by place of marriage event"),
                default: None,
                min: None,
                max: None,
                r#enum: None,
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
        ],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<record>",
                description: Some("Marriage record objects from /response/docs"),
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
