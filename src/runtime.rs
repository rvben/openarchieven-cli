//! Per-invocation runtime context.

use std::path::PathBuf;
use std::time::Duration;

use crate::cache::Cache;
use crate::cli::{ApiArgs, Cli, FormatArg};
use crate::client::{CacheMode, Client, ClientConfig, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::tty::{Format, Stream, is_tty};

#[derive(Debug, Clone)]
pub struct GlobalArgs {
    pub format: Format,
    pub quiet: bool,
    pub no_color: bool,
}

#[derive(Debug, Clone)]
pub struct ApiContext {
    pub timeout: Duration,
    pub cache_mode: CacheMode,
    pub cache_ttl_override: Option<TtlOverride>,
    pub cache_dir: Option<PathBuf>,
    pub fields: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub lang: String,
}

#[derive(Debug, Clone, Copy)]
pub enum TtlOverride {
    Disabled,
    Forever,
    Fixed(Duration),
}

impl GlobalArgs {
    pub fn from_cli(cli: &Cli) -> Self {
        let env_fmt = std::env::var("OPENARCHIEVEN_OUTPUT").ok().and_then(|s| {
            match s.to_ascii_lowercase().as_str() {
                "json" => Some(FormatArg::Json),
                "table" => Some(FormatArg::Table),
                "markdown" => Some(FormatArg::Markdown),
                _ => None,
            }
        });
        let resolved = cli
            .output
            .or(env_fmt)
            .map(|f| match f {
                FormatArg::Json => Format::Json,
                FormatArg::Table => Format::Table,
                FormatArg::Markdown => Format::Markdown,
            })
            .unwrap_or_else(|| {
                if is_tty(Stream::Stdout) {
                    Format::Table
                } else {
                    Format::Json
                }
            });
        Self {
            format: resolved,
            quiet: cli.quiet,
            no_color: cli.no_color || std::env::var_os("NO_COLOR").is_some(),
        }
    }
}

impl ApiContext {
    pub fn from_args(args: &ApiArgs) -> Result<Self> {
        if args.no_cache && args.refresh {
            return Err(Error::new(
                ErrorKind::Validation,
                "--no-cache and --refresh are mutually exclusive",
            ));
        }

        let cache_ttl_override = match args.cache_ttl.as_deref() {
            None => None,
            Some("inf") => Some(TtlOverride::Forever),
            Some("0") => Some(TtlOverride::Disabled),
            Some(s) => {
                let d = humantime::parse_duration(s).map_err(|e| {
                    Error::new(ErrorKind::Validation, format!("--cache-ttl {s}: {e}"))
                })?;
                if d.is_zero() {
                    Some(TtlOverride::Disabled)
                } else {
                    Some(TtlOverride::Fixed(d))
                }
            }
        };

        let cache_mode =
            if args.no_cache || matches!(cache_ttl_override, Some(TtlOverride::Disabled)) {
                CacheMode::Bypass
            } else if args.refresh {
                CacheMode::Refresh
            } else {
                CacheMode::Default
            };

        let fields = args.fields.as_deref().map(|s| {
            s.split(',')
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect::<Vec<_>>()
        });

        Ok(Self {
            timeout: args.timeout.unwrap_or_else(|| Duration::from_secs(30)),
            cache_mode,
            cache_ttl_override,
            cache_dir: args.cache_dir.clone(),
            fields,
            limit: args.limit,
            offset: args.offset,
            lang: args.lang.clone().unwrap_or_else(|| "nl".to_string()),
        })
    }
}

pub fn build_client(api: &ApiContext) -> Result<Client> {
    let base_url = crate::client::resolve_base_url(None);
    Client::new(ClientConfig {
        base_url,
        timeout: api.timeout,
        lang: api.lang.clone(),
        rate_limit_per_sec: 4,
        cache_mode: api.cache_mode,
    })
}

pub fn build_cache(api: &ApiContext) -> Result<Option<Cache>> {
    if matches!(api.cache_mode, CacheMode::Bypass) {
        return Ok(None);
    }
    let dir = api
        .cache_dir
        .clone()
        .or_else(default_cache_dir)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::Validation,
                "could not determine cache directory; pass --cache-dir or set OPENARCHIEVEN_CACHE_DIR",
            )
        })?;
    Cache::open(dir, false).map(Some)
}

fn default_cache_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "openarchieven").map(|p| p.cache_dir().to_path_buf())
}

pub fn resolve_ttl(ctx: &ApiContext, default: TtlHint) -> TtlHint {
    match ctx.cache_ttl_override {
        Some(TtlOverride::Disabled) => TtlHint::None,
        Some(TtlOverride::Forever) => TtlHint::Never,
        Some(TtlOverride::Fixed(d)) => TtlHint::Fixed(d),
        None => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ApiArgs;

    fn args() -> ApiArgs {
        ApiArgs {
            timeout: None,
            no_cache: false,
            refresh: false,
            cache_ttl: None,
            cache_dir: None,
            fields: None,
            limit: None,
            offset: None,
            lang: None,
            rest: vec![],
        }
    }

    #[test]
    fn no_cache_and_refresh_conflict() {
        let mut a = args();
        a.no_cache = true;
        a.refresh = true;
        let err = ApiContext::from_args(&a).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn cache_ttl_zero_means_bypass() {
        let mut a = args();
        a.cache_ttl = Some("0".into());
        let ctx = ApiContext::from_args(&a).unwrap();
        assert!(matches!(ctx.cache_mode, CacheMode::Bypass));
    }

    #[test]
    fn cache_ttl_inf_means_forever() {
        let mut a = args();
        a.cache_ttl = Some("inf".into());
        let ctx = ApiContext::from_args(&a).unwrap();
        assert!(matches!(ctx.cache_ttl_override, Some(TtlOverride::Forever)));
    }

    #[test]
    fn fields_csv_is_parsed() {
        let mut a = args();
        a.fields = Some("name, year ,place".into());
        let ctx = ApiContext::from_args(&a).unwrap();
        assert_eq!(ctx.fields.unwrap(), vec!["name", "year", "place"]);
    }

    #[test]
    fn lang_defaults_to_nl() {
        let ctx = ApiContext::from_args(&args()).unwrap();
        assert_eq!(ctx.lang, "nl");
    }

    #[test]
    fn refresh_flag_sets_refresh_mode() {
        let mut a = args();
        a.refresh = true;
        let ctx = ApiContext::from_args(&a).unwrap();
        assert!(matches!(ctx.cache_mode, CacheMode::Refresh));
    }

    #[test]
    fn cache_ttl_invalid_string_is_validation_error() {
        let mut a = args();
        a.cache_ttl = Some("notaduration".into());
        let err = ApiContext::from_args(&a).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }
}
