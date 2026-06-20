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
        "sources",
        "/stats/sources.json",
        "/sources",
        client,
        cache,
        ctx,
        args,
    )
}

pub fn schema() -> Command {
    Command {
        name: "stats sources",
        description: "Per-archive source-type counts.",
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
            description: None,
            default: None,
            min: None,
            max: None,
            r#enum: None,
        }],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<row>",
                description: None,
            },
            OutputField {
                name: "total",
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
