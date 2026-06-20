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
                description: None,
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
                description: None,
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
                description: None,
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
                description: None,
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
                description: None,
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
                description: None,
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
                description: None,
            },
            OutputField {
                name: "total",
                ty: "integer | null",
                description: None,
            },
            OutputField {
                name: "limit",
                ty: "integer",
                description: None,
            },
            OutputField {
                name: "offset",
                ty: "integer",
                description: None,
            },
            OutputField {
                name: "paginated",
                ty: "boolean",
                description: None,
            },
        ],
    }
}
