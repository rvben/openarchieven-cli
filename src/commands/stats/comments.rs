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
        "comments",
        "/stats/comments.json",
        "/comments",
        client,
        cache,
        ctx,
        args,
    )
}

pub fn schema() -> Command {
    Command {
        name: "stats comments",
        description: "Per-archive comment counts.",
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
            description: Some("Filter results to a specific archive code"),
            default: None,
            min: None,
            max: None,
            r#enum: None,
        }],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<row>",
                description: Some("Per-archive comment count rows"),
            },
            OutputField {
                name: "total",
                ty: "integer",
                description: Some("Number of rows returned"),
            },
            OutputField {
                name: "paginated",
                ty: "boolean",
                description: Some("Always false; this command is non-paginated"),
            },
        ],
    }
}
