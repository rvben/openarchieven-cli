use crate::schema_cmd::{Command, OutputField};

pub fn schema() -> Command {
    Command {
        name: "show",
        description: "<TODO>",
        mutating: false,
        response_shape: "list",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "none",
        args: vec![],
        output_fields: vec![OutputField {
            name: "items",
            ty: "array",
        }],
    }
}
