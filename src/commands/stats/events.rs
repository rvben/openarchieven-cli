use crate::cache::Cache;
use crate::client::Client;
use crate::commands::stats::archive_stat::{ArchiveStatArgs, run_archive_stat};
use crate::error::Result;
use crate::output::Renderable;
use crate::runtime::ApiContext;
use crate::schema_cmd::{Arg, Command, OutputField};

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &ArchiveStatArgs,
) -> Result<Renderable> {
    run_archive_stat(
        "events",
        "/stats/events.json",
        "/events",
        client,
        cache,
        ctx,
        args,
    )
}

pub fn schema() -> Command {
    Command {
        name: "stats events",
        description: "Per-archive event-type counts.",
        mutating: false,
        response_shape: "list",
        paginated: false,
        cache_ttl_seconds: Some(24 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![Arg {
            name: "--archive",
            ty: "string",
            required: false,
            positional: false,
            default: None,
            min: None,
            max: None,
            r#enum: None,
        }],
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
                name: "paginated",
                ty: "boolean",
            },
        ],
    }
}
