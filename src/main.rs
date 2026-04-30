use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind as ClapErrorKind;

use openarchieven::cli::{ApiArgs, CacheCmd, Cli, Cmd, StatsCmd};
use openarchieven::error::{Error, ErrorKind, emit_json};

/// Returns `true` when `NO_COLOR` is set to a non-empty value.
///
/// Per <https://no-color.org/>: any non-empty value disables color. An empty
/// string or an absent variable are treated identically (color enabled).
fn no_color_env() -> bool {
    std::env::var_os("NO_COLOR")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

fn main() -> ExitCode {
    match Cli::try_parse() {
        Ok(cli) => {
            let no_color = cli.no_color || no_color_env();
            match dispatch(cli) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    emit_error(&err, no_color);
                    ExitCode::from(err.kind().exit_code())
                }
            }
        }
        Err(clap_err) => {
            // `--help` and `--version` are not errors — let clap render them
            // to stdout and exit 0.
            if matches!(
                clap_err.kind(),
                ClapErrorKind::DisplayHelp | ClapErrorKind::DisplayVersion
            ) {
                let _ = clap_err.print();
                return ExitCode::SUCCESS;
            }
            let no_color = no_color_env();
            let err = Error::new(ErrorKind::Validation, clap_err.to_string());
            emit_error(&err, no_color);
            ExitCode::from(err.kind().exit_code())
        }
    }
}

fn emit_error(err: &Error, no_color: bool) {
    let mut stderr = std::io::stderr().lock();
    if std::io::stderr().is_terminal() && !no_color {
        let _ = writeln!(stderr, "error[{}]: {}", err.kind(), err.message());
    }
    let _ = emit_json(&mut stderr, err);
}

/// Render to stdout, treating broken pipe (downstream `head`/`less`/etc.
/// closing early) as a clean exit. Other I/O failures here mean stdout is
/// hosed and we panic — there's nothing useful we could write next.
fn write_stdout(
    f: impl FnOnce(&mut std::io::StdoutLock<'_>) -> std::io::Result<()>,
) -> Result<(), Error> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    match f(&mut handle) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => panic!("stdout write failed: {e}"),
    }
}

fn dispatch(cli: Cli) -> Result<(), Error> {
    let global = openarchieven::runtime::GlobalArgs::from_cli(&cli);
    match cli.command {
        Cmd::Version => {
            let body = serde_json::json!({ "version": env!("CARGO_PKG_VERSION") });
            let renderable = openarchieven::output::Renderable::single_flat(body);
            let pretty = std::io::stdout().is_terminal();
            write_stdout(|out| {
                openarchieven::output::render(out, &renderable, global.format, pretty)
            })
        }
        Cmd::Schema => {
            let schema = openarchieven::schema_cmd::build();
            let json = serde_json::to_string_pretty(&schema).expect("schema always serializes");
            write_stdout(|out| writeln!(out, "{json}"))
        }
        Cmd::Archives(args) => run_endpoint(args, &global, |client, cache, ctx, _rest| {
            openarchieven::commands::archives::run(client, cache, ctx)
        }),
        Cmd::Search(args) => {
            let openarchieven::cli::SearchArgs {
                global: global_api,
                name,
                archive,
                source_type,
                event_place,
                birth_place,
                relation_type,
                country,
                sort,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = openarchieven::commands::search::Args {
                    name,
                    archive,
                    source_type,
                    event_place,
                    birth_place,
                    relation_type,
                    country,
                    sort,
                };
                openarchieven::commands::search::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Show(args) => run_endpoint(args, &global, |client, cache, ctx, rest| {
            let parsed = openarchieven::commands::show::parse_rest(rest)?;
            openarchieven::commands::show::run(client, cache, ctx, &parsed)
        }),
        Cmd::MatchCmd(args) => {
            let openarchieven::cli::MatchArgs {
                global: global_api,
                name,
                birth_year,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = openarchieven::commands::match_record::Args { name, birth_year };
                openarchieven::commands::match_record::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Births(args) => {
            let openarchieven::cli::BirthsArgs {
                global: global_api,
                name,
                event_year,
                event_place,
                event_province,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = openarchieven::commands::births::Args {
                    name,
                    flags: openarchieven::commands::event_records::CommonFlags {
                        event_year,
                        event_place,
                        event_province,
                    },
                };
                openarchieven::commands::births::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Deaths(args) => {
            let openarchieven::cli::DeathsArgs {
                global: global_api,
                name,
                event_year,
                event_place,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = openarchieven::commands::deaths::Args {
                    name,
                    flags: openarchieven::commands::event_records::CommonFlags {
                        event_year,
                        event_place,
                        event_province: None,
                    },
                };
                openarchieven::commands::deaths::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Marriages(args) => {
            let openarchieven::cli::MarriagesArgs {
                global: global_api,
                name1,
                name2,
                event_year,
                event_place,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = openarchieven::commands::marriages::Args {
                    name1,
                    name2,
                    flags: openarchieven::commands::event_records::CommonFlags {
                        event_year,
                        event_place,
                        event_province: None,
                    },
                };
                openarchieven::commands::marriages::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Yearsago(args) => {
            let openarchieven::cli::YearsagoArgs {
                global: global_api,
                years,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = openarchieven::commands::yearsago::Args { years };
                openarchieven::commands::yearsago::run(client, cache, ctx, &typed)
            })
        }
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
    let dir = std::env::var_os("OPENARCHIEVEN_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .or_else(openarchieven::runtime::default_cache_dir)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::Validation,
                "could not determine cache directory; set OPENARCHIEVEN_CACHE_DIR",
            )
        })?;
    let cache = openarchieven::cache::Cache::open(dir, false)?;
    let renderable = f(&cache)?;
    let pretty = std::io::stdout().is_terminal();
    write_stdout(|out| openarchieven::output::render(out, &renderable, global.format, pretty))
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
    let mut renderable = f(&client, cache.as_ref(), &ctx, &rest)?;
    if let Some(fields) = ctx.fields.as_deref() {
        renderable = openarchieven::output::apply_fields_auto(renderable, fields)?;
    }
    let pretty = std::io::stdout().is_terminal();
    write_stdout(|out| openarchieven::output::render(out, &renderable, global.format, pretty))
}

fn run_typed_endpoint<F>(
    api_args: openarchieven::cli::GlobalApiArgs,
    global: &openarchieven::runtime::GlobalArgs,
    f: F,
) -> Result<(), Error>
where
    F: FnOnce(
        &openarchieven::client::Client,
        Option<&openarchieven::cache::Cache>,
        &openarchieven::runtime::ApiContext,
    ) -> Result<openarchieven::output::Renderable, Error>,
{
    let ctx = openarchieven::runtime::ApiContext::from_global_args(&api_args)?;
    let client = openarchieven::runtime::build_client(&ctx)?;
    let cache = openarchieven::runtime::build_cache(&ctx)?;
    let mut renderable = f(&client, cache.as_ref(), &ctx)?;
    if let Some(fields) = ctx.fields.as_deref() {
        renderable = openarchieven::output::apply_fields_auto(renderable, fields)?;
    }
    let pretty = std::io::stdout().is_terminal();
    write_stdout(|out| openarchieven::output::render(out, &renderable, global.format, pretty))
}
