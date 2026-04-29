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
    pub env_vars: Vec<EnvVar>,
    pub cache: CacheInfo,
    pub commands: Vec<Command>,
    pub errors: Vec<ErrorEntry>,
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
    pub cache_ttl_seconds: Option<u64>, // None for non-API commands or "until_midnight"
    pub cache_ttl_strategy: &'static str, // "fixed" | "until_midnight" | "none"
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
        output_formats: vec!["json", "table", "markdown"],
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

fn commands() -> Vec<Command> {
    vec![
        crate::commands::search::schema(),
        crate::commands::show::schema(),
        crate::commands::match_record::schema(),
        crate::commands::births::schema(),
        crate::commands::deaths::schema(),
        crate::commands::marriages::schema(),
        crate::commands::yearsago::schema(),
        crate::commands::archives::schema(),
        crate::commands::census::schema(),
        crate::commands::weather::schema(),
        crate::commands::stats::records::schema(),
        crate::commands::stats::sources::schema(),
        crate::commands::stats::events::schema(),
        crate::commands::stats::comments::schema(),
        crate::commands::stats::familynames::schema(),
        crate::commands::stats::firstnames::schema(),
        crate::commands::stats::professions::schema(),
        cache_info_schema(),
        cache_clear_schema(),
        cache_prune_schema(),
        schema_self(),
        version_self(),
    ]
}

fn cache_info_schema() -> Command {
    Command {
        name: "cache info",
        description: "Show cache location, total size, and entry count",
        mutating: false,
        response_shape: "single-flat",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![],
        output_fields: vec![
            OutputField {
                name: "root",
                ty: "string",
            },
            OutputField {
                name: "entries",
                ty: "integer",
            },
            OutputField {
                name: "bytes",
                ty: "integer",
            },
            OutputField {
                name: "oldest",
                ty: "datetime | null",
            },
            OutputField {
                name: "newest",
                ty: "datetime | null",
            },
        ],
    }
}

fn cache_clear_schema() -> Command {
    Command {
        name: "cache clear",
        description: "Delete all cache entries (requires --yes)",
        mutating: true,
        response_shape: "single-flat",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![Arg {
            name: "--yes",
            ty: "boolean",
            required: true,
            positional: false,
            default: None,
            min: None,
            max: None,
            r#enum: None,
        }],
        output_fields: vec![OutputField {
            name: "deleted",
            ty: "integer",
        }],
    }
}

fn cache_prune_schema() -> Command {
    Command {
        name: "cache prune",
        description: "Delete only expired cache entries",
        mutating: true,
        response_shape: "single-flat",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![],
        output_fields: vec![OutputField {
            name: "deleted",
            ty: "integer",
        }],
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

    // Until command modules exist, this stub builder is exercised in
    // tests/schema.rs (snapshot). Here we just verify the static blocks.

    #[test]
    fn errors_block_lists_eight_kinds() {
        assert_eq!(errors().len(), 8);
    }

    #[test]
    fn output_formats_are_three() {
        let s = Schema {
            name: "openarchieven",
            version: "0.1.0",
            base_url: "https://api.openarchieven.nl/1.1",
            rate_limit: RateLimit {
                requests_per_second: 4,
                scope: "per_ip",
            },
            output_formats: vec!["json", "table", "markdown"],
            env_vars: vec![],
            cache: CacheInfo {
                default_dir_template: "x",
                file_permissions: "0600",
                dir_permissions: "0700",
            },
            commands: vec![],
            errors: errors(),
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["output_formats"].as_array().unwrap().len(), 3);
    }
}
