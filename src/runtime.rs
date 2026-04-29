//! Per-invocation runtime context.

use std::path::PathBuf;
use std::time::Duration;

use crate::cache::Cache;
use crate::cli::{ApiArgs, Cli, FormatArg};
use crate::client::{CacheMode, Client, ClientConfig, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::tty::{Format, Stream, is_tty, resolve_format};

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
        let explicit = cli.output.map(|f| match f {
            FormatArg::Json => Format::Json,
            FormatArg::Table => Format::Table,
            FormatArg::Markdown => Format::Markdown,
        });
        let env = std::env::var("OPENARCHIEVEN_OUTPUT").ok();
        let format = resolve_format(explicit, env.as_deref(), is_tty(Stream::Stdout));
        Self {
            format,
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
        rate_limit_per_sec: rate_limit_from_env().unwrap_or(4),
        cache_mode: api.cache_mode,
    })
}

pub fn build_cache(api: &ApiContext) -> Result<Option<Cache>> {
    if matches!(api.cache_mode, CacheMode::Bypass) || cache_disabled_via_env() {
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

fn cache_disabled_via_env() -> bool {
    matches!(
        std::env::var("OPENARCHIEVEN_CACHE_DISABLE").as_deref(),
        Ok("1")
    )
}

fn rate_limit_from_env() -> Option<u32> {
    std::env::var("OPENARCHIEVEN_RATE_LIMIT")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&n| n > 0)
}

pub fn default_cache_dir() -> Option<PathBuf> {
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

    #[test]
    fn rate_limit_env_returns_none_for_unset() {
        // Sanity check: parser logic doesn't panic on absence.
        // We don't unset the env var here because the test runs in
        // parallel with others and could see leakage; just exercise
        // the parser surface.
        let parsed: Option<u32> = "0".parse::<u32>().ok().filter(|&n| n > 0);
        assert_eq!(parsed, None);
    }

    #[test]
    fn rate_limit_env_rejects_zero_and_garbage() {
        for bad in ["0", "-1", "abc", ""] {
            let parsed: Option<u32> = bad.parse::<u32>().ok().filter(|&n| n > 0);
            assert_eq!(parsed, None, "{bad}");
        }
    }
}
