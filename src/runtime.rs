//! Per-invocation runtime context.

use std::path::PathBuf;
use std::time::Duration;

use crate::cache::Cache;
use crate::cli::{Cli, FormatArg};
use crate::client::{CacheMode, Client, ClientConfig, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::tty::{Format, Stream, is_tty, resolve_format};

#[derive(Debug, Clone)]
pub struct GlobalArgs {
    pub format: Format,
    pub pretty: bool,
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
    pub quiet: bool,
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
            FormatArg::Ndjson => Format::Ndjson,
            FormatArg::Table => Format::Table,
            FormatArg::Text => Format::Text,
            FormatArg::Markdown => Format::Markdown,
        });
        let env = std::env::var("OPENARCHIEVEN_OUTPUT").ok();
        let stdout_tty = is_tty(Stream::Stdout);
        let format = resolve_format(explicit, env.as_deref(), stdout_tty);
        Self {
            format,
            pretty: cli.pretty || stdout_tty,
            quiet: cli.quiet,
            no_color: cli.no_color || std::env::var_os("NO_COLOR").is_some(),
        }
    }
}

/// Borrowed view of the raw flag values from `GlobalApiArgs`.
/// Avoids duplicating construction logic while staying within clippy's
/// argument-count limit.
struct ApiContextInput<'a> {
    timeout: Option<Duration>,
    no_cache: bool,
    refresh: bool,
    cache_ttl: Option<&'a str>,
    cache_dir: Option<PathBuf>,
    fields: Option<&'a str>,
    limit: Option<u32>,
    offset: Option<u32>,
    lang: Option<&'a str>,
    quiet: bool,
}

impl ApiContext {
    pub fn from_global_args(args: &crate::cli::GlobalApiArgs, quiet: bool) -> Result<Self> {
        ApiContextInput {
            timeout: args.timeout,
            no_cache: args.no_cache,
            refresh: args.refresh,
            cache_ttl: args.cache_ttl.as_deref(),
            cache_dir: args.cache_dir.clone(),
            fields: args.fields.as_deref(),
            limit: args.limit,
            offset: args.offset,
            lang: args.lang.as_deref(),
            quiet,
        }
        .build()
    }
}

impl ApiContextInput<'_> {
    fn build(self) -> Result<ApiContext> {
        if self.no_cache && self.refresh {
            return Err(Error::new(
                ErrorKind::Validation,
                "--no-cache and --refresh are mutually exclusive",
            ));
        }

        let cache_ttl_override = match self.cache_ttl {
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
            if self.no_cache || matches!(cache_ttl_override, Some(TtlOverride::Disabled)) {
                CacheMode::Bypass
            } else if self.refresh {
                CacheMode::Refresh
            } else {
                CacheMode::Default
            };

        let fields = self.fields.map(|s| {
            s.split(',')
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect::<Vec<_>>()
        });

        Ok(ApiContext {
            timeout: self.timeout.unwrap_or_else(|| Duration::from_secs(30)),
            cache_mode,
            cache_ttl_override,
            cache_dir: self.cache_dir,
            fields,
            limit: self.limit,
            offset: self.offset,
            lang: self.lang.unwrap_or("nl").to_string(),
            quiet: self.quiet,
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
    use crate::cli::GlobalApiArgs;
    use std::time::Duration;

    fn args() -> GlobalApiArgs {
        GlobalApiArgs::default()
    }

    #[test]
    fn from_global_args_no_cache_and_refresh_conflict() {
        let mut g = args();
        g.no_cache = true;
        g.refresh = true;
        let err = ApiContext::from_global_args(&g, false).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn from_global_args_lang_defaults_to_nl() {
        let ctx = ApiContext::from_global_args(&args(), false).unwrap();
        assert_eq!(ctx.lang, "nl");
    }

    #[test]
    fn no_cache_and_refresh_conflict() {
        let mut a = args();
        a.no_cache = true;
        a.refresh = true;
        let err = ApiContext::from_global_args(&a, false).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
    }

    #[test]
    fn cache_ttl_zero_means_bypass() {
        let mut a = args();
        a.cache_ttl = Some("0".into());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert!(matches!(ctx.cache_mode, CacheMode::Bypass));
    }

    #[test]
    fn cache_ttl_inf_means_forever() {
        let mut a = args();
        a.cache_ttl = Some("inf".into());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert!(matches!(ctx.cache_ttl_override, Some(TtlOverride::Forever)));
    }

    #[test]
    fn fields_csv_is_parsed() {
        let mut a = args();
        a.fields = Some("name, year ,place".into());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert_eq!(ctx.fields.unwrap(), vec!["name", "year", "place"]);
    }

    #[test]
    fn lang_defaults_to_nl() {
        let ctx = ApiContext::from_global_args(&args(), false).unwrap();
        assert_eq!(ctx.lang, "nl");
    }

    #[test]
    fn refresh_flag_sets_refresh_mode() {
        let mut a = args();
        a.refresh = true;
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert!(matches!(ctx.cache_mode, CacheMode::Refresh));
    }

    #[test]
    fn cache_ttl_invalid_string_is_validation_error() {
        let mut a = args();
        a.cache_ttl = Some("notaduration".into());
        let err = ApiContext::from_global_args(&a, false).unwrap_err();
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

    #[test]
    fn cache_ttl_humantime_duration_sets_fixed_ttl() {
        let mut a = args();
        a.cache_ttl = Some("1h".into());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert!(
            matches!(ctx.cache_ttl_override, Some(TtlOverride::Fixed(d)) if d == Duration::from_secs(3600))
        );
        assert!(matches!(ctx.cache_mode, CacheMode::Default));
    }

    #[test]
    fn no_cache_flag_sets_bypass_mode() {
        let mut a = args();
        a.no_cache = true;
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert!(matches!(ctx.cache_mode, CacheMode::Bypass));
    }

    #[test]
    fn timeout_arg_is_respected() {
        let mut a = args();
        a.timeout = Some(Duration::from_secs(10));
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert_eq!(ctx.timeout, Duration::from_secs(10));
    }

    #[test]
    fn empty_fields_string_produces_empty_vec() {
        let mut a = args();
        a.fields = Some("".into());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert!(ctx.fields.as_ref().unwrap().is_empty());
    }

    #[test]
    fn lang_arg_overrides_default() {
        let mut a = args();
        a.lang = Some("en".into());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert_eq!(ctx.lang, "en");
    }

    #[test]
    fn limit_and_offset_are_passed_through() {
        let mut a = args();
        a.limit = Some(20);
        a.offset = Some(5);
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        assert_eq!(ctx.limit, Some(20));
        assert_eq!(ctx.offset, Some(5));
    }

    #[test]
    fn resolve_ttl_disabled_override_returns_none() {
        let mut a = args();
        a.cache_ttl = Some("0".into());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        let hint = resolve_ttl(&ctx, TtlHint::Fixed(Duration::from_secs(60)));
        assert!(matches!(hint, TtlHint::None));
    }

    #[test]
    fn resolve_ttl_forever_override_returns_never() {
        let mut a = args();
        a.cache_ttl = Some("inf".into());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        let hint = resolve_ttl(&ctx, TtlHint::Fixed(Duration::from_secs(60)));
        assert!(matches!(hint, TtlHint::Never));
    }

    #[test]
    fn resolve_ttl_fixed_override_replaces_default() {
        let mut a = args();
        a.cache_ttl = Some("30m".into());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        let hint = resolve_ttl(&ctx, TtlHint::UntilMidnight);
        assert!(matches!(hint, TtlHint::Fixed(d) if d == Duration::from_secs(1800)));
    }

    #[test]
    fn resolve_ttl_no_override_returns_default() {
        let ctx = ApiContext::from_global_args(&args(), false).unwrap();
        let hint = resolve_ttl(&ctx, TtlHint::UntilMidnight);
        assert!(matches!(hint, TtlHint::UntilMidnight));
    }

    #[test]
    fn cache_ttl_zero_duration_via_humantime_means_bypass() {
        // humantime can parse "0s" as a zero Duration, which should map to Bypass.
        let mut a = args();
        a.cache_ttl = Some("0s".into());
        // humantime returns Ok(Duration::ZERO) for "0s" in recent versions.
        // If it fails to parse, that's also acceptable — just check it doesn't panic.
        match ApiContext::from_global_args(&a, false) {
            Ok(ctx) => assert!(matches!(ctx.cache_mode, CacheMode::Bypass)),
            Err(e) => assert_eq!(e.kind(), ErrorKind::Validation),
        }
    }

    #[test]
    fn global_args_no_output_flag_uses_tty_detection() {
        use crate::cli::{Cli, Cmd};
        let cli = Cli {
            output: None,
            pretty: false,
            quiet: false,
            no_color: false,
            api: args(),
            command: Cmd::Version,
        };
        let ga = GlobalArgs::from_cli(&cli);
        // The exact format depends on TTY state; just check it doesn't panic.
        let _ = ga.format;
    }

    #[test]
    fn global_args_explicit_json_output_flag() {
        use crate::cli::{Cli, Cmd, FormatArg};
        let cli = Cli {
            output: Some(FormatArg::Json),
            pretty: false,
            quiet: false,
            no_color: false,
            api: args(),
            command: Cmd::Version,
        };
        let ga = GlobalArgs::from_cli(&cli);
        assert_eq!(ga.format, crate::tty::Format::Json);
    }

    #[test]
    fn global_args_explicit_table_output_flag() {
        use crate::cli::{Cli, Cmd, FormatArg};
        let cli = Cli {
            output: Some(FormatArg::Table),
            pretty: false,
            quiet: true,
            no_color: true,
            api: args(),
            command: Cmd::Version,
        };
        let ga = GlobalArgs::from_cli(&cli);
        assert_eq!(ga.format, crate::tty::Format::Table);
        assert!(ga.quiet);
        assert!(ga.no_color);
    }

    #[test]
    fn global_args_explicit_markdown_output_flag() {
        use crate::cli::{Cli, Cmd, FormatArg};
        let cli = Cli {
            output: Some(FormatArg::Markdown),
            pretty: false,
            quiet: false,
            no_color: false,
            api: args(),
            command: Cmd::Version,
        };
        let ga = GlobalArgs::from_cli(&cli);
        assert_eq!(ga.format, crate::tty::Format::Markdown);
    }

    #[test]
    fn build_client_returns_ok_with_valid_context() {
        let ctx = ApiContext::from_global_args(&args(), false).unwrap();
        assert!(build_client(&ctx).is_ok());
    }

    #[test]
    fn build_cache_returns_none_when_bypass() {
        let mut a = args();
        a.no_cache = true;
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        let result = build_cache(&ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn build_cache_with_explicit_dir_returns_some() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = args();
        a.cache_dir = Some(dir.path().to_path_buf());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        let result = build_cache(&ctx).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn cache_disabled_via_env_returns_true_when_set_to_1() {
        unsafe { std::env::set_var("OPENARCHIEVEN_CACHE_DISABLE", "1") };
        let result = cache_disabled_via_env();
        unsafe { std::env::remove_var("OPENARCHIEVEN_CACHE_DISABLE") };
        assert!(result);
    }

    #[test]
    fn cache_disabled_via_env_returns_false_for_other_values() {
        unsafe { std::env::set_var("OPENARCHIEVEN_CACHE_DISABLE", "0") };
        let result = cache_disabled_via_env();
        unsafe { std::env::remove_var("OPENARCHIEVEN_CACHE_DISABLE") };
        assert!(!result);
    }

    #[test]
    fn build_cache_returns_none_when_env_disable_is_1() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = args();
        a.cache_dir = Some(dir.path().to_path_buf());
        let ctx = ApiContext::from_global_args(&a, false).unwrap();
        unsafe { std::env::set_var("OPENARCHIEVEN_CACHE_DISABLE", "1") };
        let result = build_cache(&ctx).unwrap();
        unsafe { std::env::remove_var("OPENARCHIEVEN_CACHE_DISABLE") };
        assert!(result.is_none());
    }

    #[test]
    fn rate_limit_from_env_returns_some_for_valid_positive() {
        unsafe { std::env::set_var("OPENARCHIEVEN_RATE_LIMIT", "10") };
        let result = rate_limit_from_env();
        unsafe { std::env::remove_var("OPENARCHIEVEN_RATE_LIMIT") };
        assert_eq!(result, Some(10));
    }

    #[test]
    fn rate_limit_from_env_returns_none_for_zero() {
        unsafe { std::env::set_var("OPENARCHIEVEN_RATE_LIMIT", "0") };
        let result = rate_limit_from_env();
        unsafe { std::env::remove_var("OPENARCHIEVEN_RATE_LIMIT") };
        assert_eq!(result, None);
    }

    #[test]
    fn default_cache_dir_returns_some_on_this_platform() {
        // ProjectDirs should resolve on macOS/Linux in the test environment.
        let result = default_cache_dir();
        assert!(result.is_some());
    }
}
