//! Static schema document — the contract between this CLI and its consumers.
//!
//! The full document is byte-stable; `tests/schema.rs` snapshots it.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Schema {
    pub name: &'static str,
    pub version: &'static str,
    pub base_url: &'static str,
    pub rate_limit: RateLimit,
    pub output_formats: Vec<&'static str>,
    pub global_args: Vec<GlobalArg>,
    pub env_vars: Vec<EnvVar>,
    pub cache: CacheInfo,
    pub commands: Vec<Command>,
    pub errors: Vec<ErrorEntry>,
}

#[derive(Debug, Serialize, Clone)]
pub struct GlobalArg {
    pub name: &'static str,
    pub short: Option<&'static str>,
    #[serde(rename = "type")]
    pub ty: &'static str,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct RateLimit {
    pub requests_per_second: u32,
    pub scope: &'static str,
}

#[derive(Debug, Serialize)]
pub struct EnvVar {
    pub name: &'static str,
    pub effect: &'static str,
}

#[derive(Debug, Serialize)]
pub struct CacheInfo {
    pub default_dir_template: &'static str,
    pub file_permissions: &'static str,
    pub dir_permissions: &'static str,
}

#[derive(Debug, Serialize, Clone)]
pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
    pub mutating: bool,
    pub response_shape: &'static str, // "list" | "single-flat" | "single-nested" | "schema" | "none"
    pub paginated: bool,
    pub cache_ttl_seconds: Option<u64>, // None for non-API commands, "until_midnight", or "never"
    pub cache_ttl_strategy: &'static str, // "fixed" | "until_midnight" | "never" | "none"
    pub args: Vec<Arg>,
    pub output_fields: Vec<OutputField>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Arg {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub ty: &'static str,
    pub required: bool,
    pub positional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#enum: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct OutputField {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub ty: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ErrorEntry {
    pub kind: &'static str,
    pub retryable: bool,
    pub exit_code: u8,
    pub fields: Vec<&'static str>,
}

pub fn build() -> Schema {
    Schema {
        name: "openarchieven",
        version: env!("CARGO_PKG_VERSION"),
        base_url: "https://api.openarchieven.nl/1.1",
        rate_limit: RateLimit {
            requests_per_second: 4,
            scope: "per_ip",
        },
        output_formats: vec!["json", "ndjson", "table", "text", "markdown"],
        global_args: global_args(),
        env_vars: vec![
            EnvVar {
                name: "OPENARCHIEVEN_BASE_URL",
                effect: "Override API base URL (test use)",
            },
            EnvVar {
                name: "OPENARCHIEVEN_OUTPUT",
                effect: "Default output format if --output not given",
            },
            EnvVar {
                name: "OPENARCHIEVEN_RATE_LIMIT",
                effect: "Override 4 req/sec local limiter (test use)",
            },
            EnvVar {
                name: "OPENARCHIEVEN_CACHE_DIR",
                effect: "Override cache directory",
            },
            EnvVar {
                name: "OPENARCHIEVEN_CACHE_DISABLE",
                effect: "When 1, disable cache for the process",
            },
            EnvVar {
                name: "NO_COLOR",
                effect: "Disable colored stderr output",
            },
        ],
        cache: CacheInfo {
            default_dir_template: "$XDG_CACHE_HOME/openarchieven (Linux), ~/Library/Caches/openarchieven (macOS)",
            file_permissions: "0600",
            dir_permissions: "0700",
        },
        commands: commands(),
        errors: errors(),
    }
}

fn errors() -> Vec<ErrorEntry> {
    vec![
        ErrorEntry {
            kind: "validation",
            retryable: false,
            exit_code: 2,
            fields: vec!["kind", "message", "upstream_code", "upstream_message"],
        },
        ErrorEntry {
            kind: "not_found",
            retryable: false,
            exit_code: 1,
            fields: vec!["kind", "message"],
        },
        ErrorEntry {
            kind: "rate_limit",
            retryable: true,
            exit_code: 1,
            fields: vec!["kind", "message", "retry_after_seconds"],
        },
        ErrorEntry {
            kind: "timeout",
            retryable: true,
            exit_code: 1,
            fields: vec!["kind", "message"],
        },
        ErrorEntry {
            kind: "network",
            retryable: true,
            exit_code: 1,
            fields: vec!["kind", "message"],
        },
        ErrorEntry {
            kind: "server",
            retryable: true,
            exit_code: 1,
            fields: vec!["kind", "message"],
        },
        ErrorEntry {
            kind: "parse",
            retryable: false,
            exit_code: 1,
            fields: vec!["kind", "message"],
        },
        ErrorEntry {
            kind: "conflict",
            retryable: false,
            exit_code: 1,
            fields: vec!["kind", "message"],
        },
    ]
}

fn global_args() -> Vec<GlobalArg> {
    vec![
        GlobalArg {
            name: "--output",
            short: Some("-o"),
            ty: "string",
            required: false,
            description: Some("Output format: json, ndjson, table, text, markdown"),
        },
        GlobalArg {
            name: "--pretty",
            short: None,
            ty: "bool",
            required: false,
            description: Some("Pretty-print JSON output"),
        },
        GlobalArg {
            name: "--quiet",
            short: Some("-q"),
            ty: "bool",
            required: false,
            description: Some("Suppress stderr progress output"),
        },
        GlobalArg {
            name: "--no-color",
            short: None,
            ty: "bool",
            required: false,
            description: Some("Disable ANSI colors"),
        },
        GlobalArg {
            name: "--no-cache",
            short: None,
            ty: "bool",
            required: false,
            description: Some("Bypass cache read and write for this invocation"),
        },
        GlobalArg {
            name: "--refresh",
            short: None,
            ty: "bool",
            required: false,
            description: Some("Bypass cache read; still write"),
        },
        GlobalArg {
            name: "--cache-ttl",
            short: None,
            ty: "string",
            required: false,
            description: Some("Override cache TTL (e.g. 1h, inf, 0)"),
        },
        GlobalArg {
            name: "--cache-dir",
            short: None,
            ty: "string",
            required: false,
            description: Some("Override cache directory"),
        },
        GlobalArg {
            name: "--fields",
            short: None,
            ty: "string",
            required: false,
            description: Some("Top-level field projection (comma-separated)"),
        },
        GlobalArg {
            name: "--limit",
            short: None,
            ty: "integer",
            required: false,
            description: Some("Pagination limit where supported"),
        },
        GlobalArg {
            name: "--offset",
            short: None,
            ty: "integer",
            required: false,
            description: Some("Pagination offset where supported"),
        },
        GlobalArg {
            name: "--lang",
            short: None,
            ty: "string",
            required: false,
            description: Some("Response language (default: nl)"),
        },
        GlobalArg {
            name: "--timeout",
            short: None,
            ty: "string",
            required: false,
            description: Some("Per-request timeout (e.g. 30s, 1m, 500ms)"),
        },
    ]
}

fn commands() -> Vec<Command> {
    vec![
        crate::commands::archives::schema(),
        init_self(),
        crate::commands::search::schema(),
        crate::commands::show::schema(),
        crate::commands::match_record::schema(),
        crate::commands::births::schema(),
        crate::commands::deaths::schema(),
        crate::commands::marriages::schema(),
        crate::commands::yearsago::schema(),
        crate::commands::census::schema(),
        crate::commands::weather::schema(),
        crate::commands::stats::records::schema(),
        crate::commands::stats::sources::schema(),
        crate::commands::stats::events::schema(),
        crate::commands::stats::comments::schema(),
        crate::commands::stats::familynames::schema(),
        crate::commands::stats::firstnames::schema(),
        crate::commands::stats::professions::schema(),
        crate::commands::stats::breakdown::schema(),
        crate::commands::transcripts::search::schema(),
        crate::commands::transcripts::browse::schema(),
        crate::commands::transcripts::show::schema(),
        crate::commands::cache_cmd::info_schema(),
        crate::commands::cache_cmd::clear_schema(),
        crate::commands::cache_cmd::prune_schema(),
        schema_self(),
        version_self(),
    ]
}

fn init_self() -> Command {
    Command {
        name: "init",
        description: "Verify the tool is installed and ready; exits 0 with a JSON confirmation",
        mutating: false,
        response_shape: "single-flat",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![],
        output_fields: vec![
            OutputField {
                name: "initialized",
                ty: "bool",
            },
            OutputField {
                name: "version",
                ty: "string",
            },
        ],
    }
}

fn schema_self() -> Command {
    Command {
        name: "schema",
        description: "Emit this machine-readable schema document",
        mutating: false,
        response_shape: "schema",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![],
        output_fields: vec![],
    }
}

fn version_self() -> Command {
    Command {
        name: "version",
        description: "Print the binary version",
        mutating: false,
        response_shape: "single-flat",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![],
        output_fields: vec![OutputField {
            name: "version",
            ty: "string",
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_block_lists_eight_kinds() {
        assert_eq!(errors().len(), 8);
    }

    #[test]
    fn build_emits_five_output_formats() {
        let s = build();
        assert_eq!(
            s.output_formats,
            vec!["json", "ndjson", "table", "text", "markdown"]
        );
    }

    #[test]
    fn build_emits_twentyseven_commands() {
        // archives + init + 17 API commands + 3 transcripts subcommands + cache info/clear/prune + schema + version.
        assert_eq!(build().commands.len(), 27);
    }

    #[test]
    fn archives_is_first_command() {
        let s = build();
        assert_eq!(s.commands[0].name, "archives");
    }

    #[test]
    fn global_args_declared() {
        let s = build();
        assert!(!s.global_args.is_empty());
        assert!(s.global_args.iter().any(|a| a.name == "--output"));
        assert!(s.global_args.iter().any(|a| a.name == "--quiet"));
    }
}
