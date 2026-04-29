use std::process::ExitCode;

use clap::Parser;

use openarchieven::cli::{ApiArgs, CacheCmd, Cli, Cmd, StatsCmd};
use openarchieven::error::{Error, ErrorKind, emit_json};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = emit_json(&mut std::io::stderr().lock(), &err);
            ExitCode::from(err.kind().exit_code())
        }
    }
}

fn dispatch(cli: Cli) -> Result<(), Error> {
    let global = openarchieven::runtime::GlobalArgs::from_cli(&cli);
    match cli.command {
        Cmd::Version => {
            println!("openarchieven {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Cmd::Schema => {
            let schema = openarchieven::schema_cmd::build();
            let json = serde_json::to_string_pretty(&schema).expect("schema always serializes");
            println!("{json}");
            Ok(())
        }
        Cmd::Archives(args) => run_endpoint(args, &global, |client, cache, ctx, _rest| {
            openarchieven::commands::archives::run(client, cache, ctx)
        }),
        Cmd::Search(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::search::parse_rest(rest)?;
            openarchieven::commands::search::run(client, cache, ctx, &parsed)
        }),
        Cmd::Show(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::show::parse_rest(rest)?;
            openarchieven::commands::show::run(client, cache, ctx, &parsed)
        }),
        Cmd::MatchCmd(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::match_record::parse_rest(rest)?;
            openarchieven::commands::match_record::run(client, cache, ctx, &parsed)
        }),
        Cmd::Births(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::births::parse_rest(rest)?;
            openarchieven::commands::births::run(client, cache, ctx, &parsed)
        }),
        Cmd::Deaths(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::deaths::parse_rest(rest)?;
            openarchieven::commands::deaths::run(client, cache, ctx, &parsed)
        }),
        Cmd::Marriages(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::marriages::parse_rest(rest)?;
            openarchieven::commands::marriages::run(client, cache, ctx, &parsed)
        }),
        Cmd::Yearsago(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::yearsago::parse_rest(rest)?;
            openarchieven::commands::yearsago::run(client, cache, ctx, &parsed)
        }),
        Cmd::Census(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::census::parse_rest(rest)?;
            openarchieven::commands::census::run(client, cache, ctx, &parsed)
        }),
        Cmd::Weather(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::weather::parse_rest(rest)?;
            openarchieven::commands::weather::run(client, cache, ctx, &parsed)
        }),
        Cmd::Stats(StatsCmd::Records(args)) => {
            run_endpoint(args, &global, |client, cache, ctx, rest| {
                let parsed = openarchieven::commands::stats::records::parse_rest(rest)?;
                openarchieven::commands::stats::records::run(client, cache, ctx, &parsed)
            })
        }
        Cmd::Stats(StatsCmd::Sources(args)) => {
            run_endpoint(args, &global, |client, cache, ctx, rest| {
                let parsed = openarchieven::commands::stats::sources::parse_rest(rest)?;
                openarchieven::commands::stats::sources::run(client, cache, ctx, &parsed)
            })
        }
        Cmd::Stats(StatsCmd::Events(args)) => {
            run_endpoint(args, &global, |client, cache, ctx, rest| {
                let parsed = openarchieven::commands::stats::events::parse_rest(rest)?;
                openarchieven::commands::stats::events::run(client, cache, ctx, &parsed)
            })
        }
        Cmd::Stats(StatsCmd::Comments(args)) => {
            run_endpoint(args, &global, |client, cache, ctx, rest| {
                let parsed = openarchieven::commands::stats::comments::parse_rest(rest)?;
                openarchieven::commands::stats::comments::run(client, cache, ctx, &parsed)
            })
        }
        Cmd::Stats(StatsCmd::Familynames(args)) => {
            run_endpoint(args, &global, |client, cache, ctx, rest| {
                let parsed = openarchieven::commands::stats::familynames::parse_rest(rest)?;
                openarchieven::commands::stats::familynames::run(client, cache, ctx, &parsed)
            })
        }
        Cmd::Stats(StatsCmd::Firstnames(args)) => {
            run_endpoint(args, &global, |client, cache, ctx, rest| {
                let parsed = openarchieven::commands::stats::firstnames::parse_rest(rest)?;
                openarchieven::commands::stats::firstnames::run(client, cache, ctx, &parsed)
            })
        }
        Cmd::Stats(StatsCmd::Professions(args)) => {
            run_endpoint(args, &global, |client, cache, ctx, rest| {
                let parsed = openarchieven::commands::stats::professions::parse_rest(rest)?;
                openarchieven::commands::stats::professions::run(client, cache, ctx, &parsed)
            })
        }
        Cmd::Cache(CacheCmd::Info) => run_cache_op(&global, |cache| {
            openarchieven::commands::cache_cmd::info(cache)
        }),
        Cmd::Cache(CacheCmd::Clear { yes }) => run_cache_op(&global, move |cache| {
            openarchieven::commands::cache_cmd::clear(cache, yes)
        }),
        Cmd::Cache(CacheCmd::Prune) => run_cache_op(&global, |cache| {
            openarchieven::commands::cache_cmd::prune(cache)
        }),
    }
}

fn run_cache_op<F>(global: &openarchieven::runtime::GlobalArgs, f: F) -> Result<(), Error>
where
    F: FnOnce(&openarchieven::cache::Cache) -> Result<openarchieven::output::Renderable, Error>,
{
    let dir = openarchieven::runtime::default_cache_dir().ok_or_else(|| {
        Error::new(
            ErrorKind::Validation,
            "could not determine cache directory; set OPENARCHIEVEN_CACHE_DIR",
        )
    })?;
    let cache = openarchieven::cache::Cache::open(dir, false)?;
    let renderable = f(&cache)?;
    openarchieven::output::render(
        &mut std::io::stdout().lock(),
        &renderable,
        global.format,
        false,
    )
    .map_err(|e| Error::new(ErrorKind::Io, e.to_string()))?;
    Ok(())
}

fn run_endpoint<F>(
    args: ApiArgs,
    global: &openarchieven::runtime::GlobalArgs,
    f: F,
) -> Result<(), Error>
where
    F: FnOnce(
        &openarchieven::client::Client,
        Option<&openarchieven::cache::Cache>,
        &openarchieven::runtime::ApiContext,
        &[String],
    ) -> Result<openarchieven::output::Renderable, Error>,
{
    let ctx = openarchieven::runtime::ApiContext::from_args(&args)?;
    let rest = args.rest;
    let client = openarchieven::runtime::build_client(&ctx)?;
    let cache = openarchieven::runtime::build_cache(&ctx)?;
    let renderable = f(&client, cache.as_ref(), &ctx, &rest)?;
    openarchieven::output::render(
        &mut std::io::stdout().lock(),
        &renderable,
        global.format,
        false,
    )
    .map_err(|e| Error::new(ErrorKind::Io, e.to_string()))?;
    Ok(())
}
