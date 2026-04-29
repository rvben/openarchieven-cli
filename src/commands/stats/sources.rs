use crate::cache::Cache;
use crate::client::Client;
use crate::commands::stats::archive_stat::{ArchiveStatArgs, parse_archive_rest, run_archive_stat};
use crate::error::Result;
use crate::output::Renderable;
use crate::runtime::ApiContext;
use crate::schema_cmd::{Arg, Command, OutputField};

pub fn parse_rest(rest: &[String]) -> Result<ArchiveStatArgs> {
    parse_archive_rest(rest, "sources")
}

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
