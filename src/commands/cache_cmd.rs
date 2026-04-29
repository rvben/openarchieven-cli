use crate::schema_cmd::{Arg, Command, OutputField};

pub fn info_schema() -> Command {
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

pub fn clear_schema() -> Command {
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

pub fn prune_schema() -> Command {
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
