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
    Search(SearchArgs),
    /// Show a single record by archive + identifier.
    Show(ShowArgs),
    /// Score-matched record lookup.
    #[command(name = "match")]
    MatchCmd(MatchArgs),
    /// Birth-event records.
    Births(BirthsArgs),
    /// Death-event records.
    Deaths(DeathsArgs),
    /// Marriage-event records.
    Marriages(MarriagesArgs),
    /// Anniversary records.
    Yearsago(YearsagoArgs),
    /// List archives.
    Archives(ArchivesArgs),
    /// Census records by place/year.
    Census(CensusArgs),
    /// Historical weather observations.
    Weather(WeatherArgs),

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

const YEARSAGO_EXAMPLES: &str = "\
Examples:
  openarchieven yearsago 100      # records from 100 years ago today
  openarchieven yearsago 50 --limit 20
";

#[derive(Debug, clap::Args)]
#[command(after_help = YEARSAGO_EXAMPLES)]
pub struct YearsagoArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,
    /// Number of years ago.
    pub years: u32,
}

#[derive(Debug, clap::Args)]
#[command(after_help = "\
Examples:
  openarchieven archives
  openarchieven -o json archives | jq '.items[] | .archive_code' | head
")]
pub struct ArchivesArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,
}

const CENSUS_EXAMPLES: &str = "\
Examples:
  openarchieven census --place Amsterdam --year 1899
  openarchieven census --place Rotterdam --year 1909 --richness 3
";

#[derive(Debug, clap::Args)]
#[command(after_help = CENSUS_EXAMPLES)]
pub struct CensusArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,
    /// Year (YYYY).
    #[arg(long)]
    pub year: i32,
    #[arg(long)]
    pub place: Option<String>,
    #[arg(long)]
    pub gg_uri: Option<String>,
    #[arg(long)]
    pub province: Option<String>,
    /// Detail level: 1, 2, or 3 (3 = most detailed).
    #[arg(long, value_parser = clap::value_parser!(i32).range(1..=3))]
    pub richness: Option<i32>,
}

const WEATHER_EXAMPLES: &str = "\
Examples:
  openarchieven weather --date 1953-02-01 --latitude 51.83 --longitude 3.91
  openarchieven -o json weather --date 1944-09-17 --latitude 51.98 --longitude 5.91 --lang en
";

fn parse_decimal_str(s: &str) -> Result<String, String> {
    s.parse::<f64>()
        .map(|_| s.to_owned())
        .map_err(|_| format!("must be a decimal number, got {s:?}"))
}

#[derive(Debug, clap::Args)]
#[command(after_help = WEATHER_EXAMPLES)]
pub struct WeatherArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,
    /// Date as YYYY-MM-DD.
    #[arg(long)]
    pub date: String,
    /// Decimal latitude.
    #[arg(long, value_parser = parse_decimal_str)]
    pub latitude: String,
    /// Decimal longitude.
    #[arg(long, value_parser = parse_decimal_str)]
    pub longitude: String,
}

const SHOW_EXAMPLES: &str = "\
Examples:
  openarchieven show srt EC1E458F-AEF6-45FB-B184-656B765BE973
  openarchieven -o json show elo abc123 | jq '.Person'
";

#[derive(Debug, clap::Args)]
#[command(after_help = SHOW_EXAMPLES)]
pub struct ShowArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,
    /// Archive code (e.g. `srt`, `elo`, `saa`). List with `openarchieven archives`.
    pub archive: String,
    /// Record identifier within that archive.
    pub identifier: String,
}

const MATCH_EXAMPLES: &str = "\
Examples:
  openarchieven match \"Pieter Jansen\" 1898
  openarchieven -o json match \"Anna de Vries\" 1925 | jq '.items[0]'
";

#[derive(Debug, clap::Args)]
#[command(after_help = MATCH_EXAMPLES)]
pub struct MatchArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,
    /// Person name to match.
    pub name: String,
    /// Birth year (YYYY).
    pub birthyear: i32,
}

const SEARCH_EXAMPLES: &str = "\
Examples:
  openarchieven search \"Pieter Jansen\"
  openarchieven search \"Jansen\" --event-place Rotterdam --limit 50
  openarchieven search \"Anna\" --archive elo --source-type \"BS Geboorte\"
  openarchieven -o json search \"Jansen\" | jq '.items[0]'
";

fn parse_sort_arg(s: &str) -> Result<i32, String> {
    let n: i32 = s.parse().map_err(|_| format!("not an integer: {s}"))?;
    if n == 0 || !(-6..=6).contains(&n) {
        return Err(format!("must be in -6..=-1 or 1..=6, got {n}"));
    }
    Ok(n)
}

#[derive(Debug, clap::Args)]
#[command(after_help = SEARCH_EXAMPLES)]
pub struct SearchArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,
    /// Free-text query (typically a person name).
    pub name: String,
    /// Filter by archive code.
    #[arg(long)]
    pub archive: Option<String>,
    /// Filter by source type (e.g. `BS Geboorte`).
    #[arg(long)]
    pub source_type: Option<String>,
    /// Filter by event place.
    #[arg(long)]
    pub event_place: Option<String>,
    /// Filter by birth place.
    #[arg(long)]
    pub birth_place: Option<String>,
    /// Filter by relation type (e.g. `vader`, `moeder`).
    #[arg(long)]
    pub relation_type: Option<String>,
    /// Filter by country.
    #[arg(long)]
    pub country: Option<String>,
    /// Sort order: -6..=-1 or 1..=6 (see `openarchieven schema` for meanings).
    #[arg(long, allow_hyphen_values = true, value_parser = parse_sort_arg)]
    pub sort: Option<i32>,
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

const DEATHS_EXAMPLES: &str = "\
Examples:
  openarchieven deaths \"Anna de Vries\" --event-year 1918 --event-place Amsterdam
  openarchieven -o json deaths \"Jansen\" --limit 50 | jq '.total'
";

#[derive(Debug, clap::Args)]
#[command(after_help = DEATHS_EXAMPLES)]
pub struct DeathsArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,
    /// Deceased's name.
    pub name: String,
    /// Filter by year of death (YYYY).
    #[arg(long)]
    pub event_year: Option<i32>,
    /// Filter by place of death.
    #[arg(long)]
    pub event_place: Option<String>,
}

const MARRIAGES_EXAMPLES: &str = "\
Examples:
  openarchieven marriages \"Pieter Jansen\" \"Anna de Vries\" --event-year 1925
  openarchieven marriages \"Hendriks\" \"Bakker\" --event-place Utrecht --limit 25
";

#[derive(Debug, clap::Args)]
#[command(after_help = MARRIAGES_EXAMPLES)]
pub struct MarriagesArgs {
    #[command(flatten)]
    pub global: GlobalApiArgs,
    /// First partner's name.
    pub name1: String,
    /// Second partner's name.
    pub name2: String,
    /// Filter by year of marriage (YYYY).
    #[arg(long)]
    pub event_year: Option<i32>,
    /// Filter by place of marriage.
    #[arg(long)]
    pub event_place: Option<String>,
}

/// Catch-all positional + flag holder for the `Stats` sub-subcommands.
/// Each command's `run()` function validates `args.rest` directly; clap
/// rejects nothing here. This struct will be removed once every Stats
/// endpoint has its own typed `clap::Args` struct.
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
