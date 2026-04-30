use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind as ClapErrorKind;

use crate::cli::{CacheCmd, Cli, Cmd, StatsCmd};
use crate::error::{Error, ErrorKind, emit_json};

/// Returns `true` when `NO_COLOR` is set to a non-empty value.
///
/// Per <https://no-color.org/>: any non-empty value disables color. An empty
/// string or an absent variable are treated identically (color enabled).
fn no_color_env() -> bool {
    std::env::var_os("NO_COLOR")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

pub fn main_entry() -> ExitCode {
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
    let global = crate::runtime::GlobalArgs::from_cli(&cli);
    match cli.command {
        Cmd::Version => {
            let body = serde_json::json!({ "version": env!("CARGO_PKG_VERSION") });
            let renderable = crate::output::Renderable::single_flat(body);
            let pretty = std::io::stdout().is_terminal();
            write_stdout(|out| crate::output::render(out, &renderable, global.format, pretty))
        }
        Cmd::Schema => {
            let schema = crate::schema_cmd::build();
            let json = serde_json::to_string_pretty(&schema).expect("schema always serializes");
            write_stdout(|out| writeln!(out, "{json}"))
        }
        Cmd::Archives(args) => {
            let crate::cli::ArchivesArgs { global: global_api } = args;
            run_typed_endpoint(global_api, &global, |client, cache, ctx| {
                crate::commands::archives::run(client, cache, ctx)
            })
        }
        Cmd::Search(args) => {
            let crate::cli::SearchArgs {
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
                let typed = crate::commands::search::Args {
                    name,
                    archive,
                    source_type,
                    event_place,
                    birth_place,
                    relation_type,
                    country,
                    sort,
                };
                crate::commands::search::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Show(args) => {
            let crate::cli::ShowArgs {
                global: global_api,
                archive,
                identifier,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::show::Args {
                    archive,
                    identifier,
                };
                crate::commands::show::run(client, cache, ctx, &typed)
            })
        }
        Cmd::MatchCmd(args) => {
            let crate::cli::MatchArgs {
                global: global_api,
                name,
                birthyear,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::match_record::Args { name, birthyear };
                crate::commands::match_record::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Births(args) => {
            let crate::cli::BirthsArgs {
                global: global_api,
                name,
                event_year,
                event_place,
                event_province,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::births::Args {
                    name,
                    flags: crate::commands::event_records::CommonFlags {
                        event_year,
                        event_place,
                        event_province,
                    },
                };
                crate::commands::births::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Deaths(args) => {
            let crate::cli::DeathsArgs {
                global: global_api,
                name,
                event_year,
                event_place,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::deaths::Args {
                    name,
                    flags: crate::commands::event_records::CommonFlags {
                        event_year,
                        event_place,
                        event_province: None,
                    },
                };
                crate::commands::deaths::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Marriages(args) => {
            let crate::cli::MarriagesArgs {
                global: global_api,
                name1,
                name2,
                event_year,
                event_place,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::marriages::Args {
                    name1,
                    name2,
                    flags: crate::commands::event_records::CommonFlags {
                        event_year,
                        event_place,
                        event_province: None,
                    },
                };
                crate::commands::marriages::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Yearsago(args) => {
            let crate::cli::YearsagoArgs {
                global: global_api,
                years,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::yearsago::Args { years };
                crate::commands::yearsago::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Census(args) => {
            let crate::cli::CensusArgs {
                global: global_api,
                year,
                place,
                gg_uri,
                province,
                richness,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::census::Args {
                    year,
                    place,
                    gg_uri,
                    province,
                    richness,
                };
                crate::commands::census::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Weather(args) => {
            let crate::cli::WeatherArgs {
                global: global_api,
                date,
                latitude,
                longitude,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::weather::Args {
                    date,
                    latitude,
                    longitude,
                };
                crate::commands::weather::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Stats(StatsCmd::Records(args)) => {
            let crate::cli::StatsArchiveArgs {
                global: global_api,
                archive,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::stats::archive_stat::ArchiveStatArgs { archive };
                crate::commands::stats::records::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Stats(StatsCmd::Sources(args)) => {
            let crate::cli::StatsArchiveArgs {
                global: global_api,
                archive,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::stats::archive_stat::ArchiveStatArgs { archive };
                crate::commands::stats::sources::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Stats(StatsCmd::Events(args)) => {
            let crate::cli::StatsArchiveArgs {
                global: global_api,
                archive,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::stats::archive_stat::ArchiveStatArgs { archive };
                crate::commands::stats::events::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Stats(StatsCmd::Comments(args)) => {
            let crate::cli::StatsArchiveArgs {
                global: global_api,
                archive,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::stats::archive_stat::ArchiveStatArgs { archive };
                crate::commands::stats::comments::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Stats(StatsCmd::Familynames(args)) => {
            let crate::cli::StatsFamilynamesArgs {
                global: global_api,
                place,
                year_start,
                year_end,
                event_type,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::stats::familynames::Args {
                    place,
                    year_start,
                    year_end,
                    event_type,
                };
                crate::commands::stats::familynames::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Stats(StatsCmd::Firstnames(args)) => {
            let crate::cli::StatsFirstnamesArgs {
                global: global_api,
                place,
                year,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::stats::firstnames::Args { place, year };
                crate::commands::stats::firstnames::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Stats(StatsCmd::Professions(args)) => {
            let crate::cli::StatsProfessionsArgs {
                global: global_api,
                place,
                year_start,
                year_end,
            } = args;
            run_typed_endpoint(global_api, &global, move |client, cache, ctx| {
                let typed = crate::commands::stats::professions::Args {
                    place,
                    year_start,
                    year_end,
                };
                crate::commands::stats::professions::run(client, cache, ctx, &typed)
            })
        }
        Cmd::Cache(CacheCmd::Info) => run_cache_op(&global, crate::commands::cache_cmd::info),
        Cmd::Cache(CacheCmd::Clear { yes }) => run_cache_op(&global, move |cache| {
            crate::commands::cache_cmd::clear(cache, yes)
        }),
        Cmd::Cache(CacheCmd::Prune) => run_cache_op(&global, crate::commands::cache_cmd::prune),
    }
}

fn run_cache_op<F>(global: &crate::runtime::GlobalArgs, f: F) -> Result<(), Error>
where
    F: FnOnce(&crate::cache::Cache) -> Result<crate::output::Renderable, Error>,
{
    let dir = std::env::var_os("OPENARCHIEVEN_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .or_else(crate::runtime::default_cache_dir)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::Validation,
                "could not determine cache directory; set OPENARCHIEVEN_CACHE_DIR",
            )
        })?;
    let cache = crate::cache::Cache::open(dir, false)?;
    let renderable = f(&cache)?;
    let pretty = std::io::stdout().is_terminal();
    write_stdout(|out| crate::output::render(out, &renderable, global.format, pretty))
}

fn run_typed_endpoint<F>(
    api_args: crate::cli::GlobalApiArgs,
    global: &crate::runtime::GlobalArgs,
    f: F,
) -> Result<(), Error>
where
    F: FnOnce(
        &crate::client::Client,
        Option<&crate::cache::Cache>,
        &crate::runtime::ApiContext,
    ) -> Result<crate::output::Renderable, Error>,
{
    let ctx = crate::runtime::ApiContext::from_global_args(&api_args, global.quiet)?;
    let client = crate::runtime::build_client(&ctx)?;
    let cache = crate::runtime::build_cache(&ctx)?;
    let mut renderable = f(&client, cache.as_ref(), &ctx)?;
    if let Some(fields) = ctx.fields.as_deref() {
        renderable = crate::output::apply_fields_auto(renderable, fields)?;
    }
    let pretty = std::io::stdout().is_terminal();
    write_stdout(|out| crate::output::render(out, &renderable, global.format, pretty))
}
