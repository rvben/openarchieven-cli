use crate::cache::Cache;
use crate::client::Client;
use crate::commands::event_records::{CommonFlags, Endpoint, parse_common_flags, run_event};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::ApiContext;
use crate::schema_cmd::{Arg, Command, OutputField};

#[derive(Debug)]
pub struct Args {
    pub name1: String,
    pub name2: String,
    pub flags: CommonFlags,
}

pub fn parse_rest(rest: &[String]) -> Result<Args> {
    let (positional, flags) = parse_common_flags(rest, false, "marriages")?;
    if positional.len() != 2 {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "marriages: expected <name1> <name2>, got {} positional argument(s)",
                positional.len()
            ),
        ));
    }
    Ok(Args {
        name1: positional[0].clone(),
        name2: positional[1].clone(),
        flags,
    })
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    run_event(
        Endpoint {
            command_name: "marriages",
            path: "/records/getMarriages.json",
            allow_province: false,
        },
        client,
        cache,
        ctx,
        ("name", args.name1.as_str()),
        Some(("name2", args.name2.as_str())),
        &args.flags,
    )
}

pub fn schema() -> Command {
    Command {
        name: "marriages",
        description: "Marriage-event records by both partners' names.",
        mutating: false,
        response_shape: "list",
        paginated: true,
        cache_ttl_seconds: Some(6 * 3600),
        cache_ttl_strategy: "fixed",
        args: vec![
            Arg {
                name: "name1",
                ty: "string",
                required: true,
                positional: true,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "name2",
                ty: "string",
                required: true,
                positional: true,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--event-year",
                ty: "integer",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--event-place",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--limit",
                ty: "integer",
                required: false,
                positional: false,
                default: Some(serde_json::json!(10)),
                min: None,
                max: Some(100),
                r#enum: None,
            },
            Arg {
                name: "--offset",
                ty: "integer",
                required: false,
                positional: false,
                default: Some(serde_json::json!(0)),
                min: None,
                max: None,
                r#enum: None,
            },
        ],
        output_fields: vec![
            OutputField {
                name: "items",
                ty: "array<record>",
            },
            OutputField {
                name: "total",
                ty: "integer | null",
            },
            OutputField {
                name: "limit",
                ty: "integer",
            },
            OutputField {
                name: "offset",
                ty: "integer",
            },
            OutputField {
                name: "paginated",
                ty: "boolean",
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn strs(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn two_positionals_ok() {
        let a = parse_rest(&strs(&["Jan", "Anna"])).unwrap();
        assert_eq!(a.name1, "Jan");
        assert_eq!(a.name2, "Anna");
    }

    #[test]
    fn one_positional_is_validation_error() {
        let err = parse_rest(&strs(&["Jan"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("marriages"));
    }

    #[test]
    fn three_positionals_is_validation_error() {
        let err = parse_rest(&strs(&["Jan", "Anna", "extra"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("marriages"));
    }

    #[test]
    fn zero_positionals_is_validation_error() {
        let err = parse_rest(&[]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("marriages"));
    }

    #[test]
    fn event_province_rejected() {
        let err = parse_rest(&strs(&["Jan", "Anna", "--event-province=ZH"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(
            err.message().contains("--event-province"),
            "message: {}",
            err.message()
        );
    }

    #[test]
    fn event_year_eq_form_parses() {
        let a = parse_rest(&strs(&["Jan", "Anna", "--event-year=1900"])).unwrap();
        assert_eq!(a.flags.event_year, Some(1900));
    }

    #[test]
    fn event_year_non_integer_is_validation_error() {
        let err = parse_rest(&strs(&["Jan", "Anna", "--event-year=abc"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--event-year"));
    }

    #[test]
    fn unknown_flag_is_validation_error() {
        let err = parse_rest(&strs(&["Jan", "Anna", "--unknown"])).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.message().contains("--unknown"));
    }
}
