use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
#[value(rename_all = "lowercase")]
pub enum FormatArg {
    Json,
    Table,
    Markdown,
}

#[derive(Debug, Parser)]
#[command(
    name = "openarchieven",
    version,
    about = "Agent-friendly CLI for the openarchieven.nl Dutch genealogical API",
    propagate_version = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Output format. Defaults: TTY → table, pipe → json.
    #[arg(global = true, long, short = 'o', value_enum)]
    pub output: Option<FormatArg>,

    /// Suppress stderr progress output.
    #[arg(global = true, long, short = 'q')]
    pub quiet: bool,

    /// Disable ANSI colors. Also via NO_COLOR env (any non-empty value).
    #[arg(global = true, long)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Free-text record search.
    Search(ApiArgs),
    /// Show a single record by archive + identifier.
    Show(ApiArgs),
    /// Score-matched record lookup.
    #[command(name = "match")]
    MatchCmd(ApiArgs),
    /// Birth-event records.
    Births(BirthsArgs),
    /// Death-event records.
    Deaths(ApiArgs),
    /// Marriage-event records.
    Marriages(ApiArgs),
    /// Anniversary records.
    Yearsago(ApiArgs),
    /// List archives.
    Archives(ApiArgs),
    /// Census records by place/year.
    Census(ApiArgs),
    /// Historical weather observations.
    Weather(ApiArgs),

    /// Aggregate statistics endpoints.
    #[command(subcommand)]
    Stats(StatsCmd),

    /// Cache management.
    #[command(subcommand)]
    Cache(CacheCmd),

    /// Print the machine-readable schema.
    Schema,

    /// Print the binary version.
    Version,
}

#[derive(Debug, Subcommand)]
pub enum StatsCmd {
    /// Aggregate record counts by archive.
    Records(ApiArgs),
    /// Aggregate source counts by archive.
    Sources(ApiArgs),
    /// Aggregate event counts by archive.
    Events(ApiArgs),
    /// Aggregate comment counts by archive.
    Comments(ApiArgs),
    /// Family-name frequency stats.
    Familynames(ApiArgs),
    /// First-name frequency stats.
    Firstnames(ApiArgs),
    /// Profession frequency stats.
    Professions(ApiArgs),
}

#[derive(Debug, Subcommand)]
pub enum CacheCmd {
    /// Show cache location, entry count, and disk usage.
    Info,
    /// Wipe all cache entries (requires --yes).
    Clear {
        #[arg(long)]
        yes: bool,
    },
    /// Drop expired entries.
    Prune,
}

/// Per-command flags shared by every endpoint subcommand. Flatten into each
/// subcommand's typed `Args` struct via `#[command(flatten)]`.
#[derive(Debug, clap::Args)]
pub struct GlobalApiArgs {
    /// Per-request timeout (humantime: `30s`, `1m`, `500ms`).
    #[arg(long, value_parser = humantime::parse_duration)]
    pub timeout: Option<Duration>,

    /// Bypass cache read AND write for this invocation.
    #[arg(long)]
    pub no_cache: bool,

    /// Bypass cache read; still write.
    #[arg(long)]
    pub refresh: bool,

    /// Override cache TTL for this invocation. `inf` = never expire.
    #[arg(long)]
    pub cache_ttl: Option<String>,

    /// Override cache directory.
    #[arg(long, env = "OPENARCHIEVEN_CACHE_DIR")]
    pub cache_dir: Option<PathBuf>,

    /// Top-level field projection (comma-separated).
    #[arg(long)]
    pub fields: Option<String>,

    /// Pagination limit (where supported).
    #[arg(long)]
    pub limit: Option<u32>,

    /// Pagination offset (where supported).
    #[arg(long)]
    pub offset: Option<u32>,

    /// Response language.
    #[arg(long)]
    pub lang: Option<String>,
}

const BIRTHS_EXAMPLES: &str = "\
Examples:
  openarchieven births \"Pieter Jansen\" --event-year 1898 --event-place Rotterdam
  openarchieven births \"de Vries\" --event-province ZH --limit 50
  openarchieven -o json births \"Jansen\" | jq '.items[] | .personname'
";

#[derive(Debug, clap::Args)]
#[command(after_help = BIRTHS_EXAMPLES)]
pub struct BirthsArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,

    /// Person name to search for (given name and/or family name).
    pub name: String,

    /// Filter by event year (YYYY).
    #[arg(long)]
    pub event_year: Option<i32>,

    /// Filter by place of event (e.g. `Rotterdam`).
    #[arg(long)]
    pub event_place: Option<String>,

    /// Filter by province (e.g. `ZH`, `NH`, `UT`).
    #[arg(long)]
    pub event_province: Option<String>,
}

/// Catch-all positional + flag holder for endpoint commands. Each command's
/// `run()` function does its own validation against `args.rest` — clap rejects
/// nothing here. In later phases each endpoint will graduate to a dedicated
/// `clap::Args` struct with typed fields.
#[derive(Debug, clap::Args)]
pub struct ApiArgs {
    /// Per-request timeout (humantime: 30s, 1m, 500ms).
    #[arg(global = true, long, value_parser = humantime::parse_duration)]
    pub timeout: Option<Duration>,

    /// Bypass cache read AND write for this invocation.
    #[arg(global = true, long)]
    pub no_cache: bool,

    /// Bypass cache read; still write.
    #[arg(global = true, long)]
    pub refresh: bool,

    /// Override cache TTL for this invocation. `inf` = never expire.
    #[arg(global = true, long)]
    pub cache_ttl: Option<String>,

    /// Override cache directory.
    #[arg(global = true, long, env = "OPENARCHIEVEN_CACHE_DIR")]
    pub cache_dir: Option<PathBuf>,

    /// Top-level field projection (comma-separated).
    #[arg(global = true, long)]
    pub fields: Option<String>,

    /// Pagination limit (where supported).
    #[arg(global = true, long)]
    pub limit: Option<u32>,

    /// Pagination offset (where supported).
    #[arg(global = true, long)]
    pub offset: Option<u32>,

    /// Response language.
    #[arg(global = true, long)]
    pub lang: Option<String>,

    /// All remaining positional + flag arguments are deferred to the
    /// command's own validator.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}
