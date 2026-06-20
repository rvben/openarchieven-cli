use crate::cache::Cache;
use crate::client::Client;
use crate::commands::event_records::{CommonFlags, Endpoint, run_event};
use crate::error::Result;
use crate::output::Renderable;
use crate::runtime::ApiContext;
use crate::schema_cmd::{Arg, Command, OutputField};

#[derive(Debug)]
pub struct Args {
    pub name: String,
    pub flags: CommonFlags,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    run_event(
        Endpoint {
            command_name: "births",
            path: "/records/getBirths.json",
            allow_province: true,
        },
        client,
        cache,
        ctx,
        ("name", args.name.as_str()),
        None,
        &args.flags,
    )
}

pub fn schema() -> Command {
    Command {
        name: "births",
        description: "Birth-event records by name with optional location/year filters.",
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
                name: "--event-province",
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
