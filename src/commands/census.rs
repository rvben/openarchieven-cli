use crate::cache::Cache;
use crate::client::{Client, TtlHint};
use crate::error::{Error, ErrorKind, Result};
use crate::output::Renderable;
use crate::runtime::{ApiContext, resolve_ttl};
use crate::schema_cmd::{Arg, Command};

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub year: i32,
    pub place: Option<String>,
    pub gg_uri: Option<String>,
    pub province: Option<String>,
    pub richness: Option<i32>,
}

pub fn run(
    client: &Client,
    cache: Option<&Cache>,
    ctx: &ApiContext,
    args: &Args,
) -> Result<Renderable> {
    if ctx.fields.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--fields is not supported for `census` (single-nested shape); use `-o json | jq`",
        ));
    }
    if ctx.limit.is_some() || ctx.offset.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "--limit/--offset are not supported by `census`",
        ));
    }
    if args.place.is_some() == args.gg_uri.is_some() {
        return Err(Error::new(
            ErrorKind::Validation,
            "exactly one of --place or --gg-uri is required",
        ));
    }

    let year_s = args.year.to_string();
    let richness_s = args.richness.map(|r| r.to_string());
    let mut params: Vec<(&str, &str)> = vec![("year", year_s.as_str())];
    if let Some(p) = args.place.as_deref() {
        params.push(("place", p));
    }
    if let Some(u) = args.gg_uri.as_deref() {
        params.push(("gg_uri", u));
    }
    if let Some(pr) = args.province.as_deref() {
        params.push(("province", pr));
    }
    if let Some(r) = richness_s.as_deref() {
        params.push(("richness", r));
    }

    let ttl = resolve_ttl(ctx, TtlHint::Never);
    let body = client.get_cached("/related/census.json", &params, ttl, cache)?;
    Ok(Renderable::single_nested(body))
}

pub fn schema() -> Command {
    Command {
        name: "census",
        description: "Census records by place or gg URI.",
        mutating: false,
        response_shape: "single-nested",
        paginated: false,
        cache_ttl_seconds: None,
        cache_ttl_strategy: "never",
        args: vec![
            Arg {
                name: "--year",
                ty: "integer",
                required: true,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--place",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--gg-uri",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--province",
                ty: "string",
                required: false,
                positional: false,
                default: None,
                min: None,
                max: None,
                r#enum: None,
            },
            Arg {
                name: "--richness",
                ty: "integer",
                required: false,
                positional: false,
                default: None,
                min: Some(1),
                max: Some(3),
                r#enum: Some(vec![
                    serde_json::json!({"value": 1, "label": "basic", "description": "Basic information and population number"}),
                    serde_json::json!({"value": 2, "label": "full", "description": "Like basic, plus all available census data"}),
                    serde_json::json!({"value": 3, "label": "aggregated", "description": "Like full, plus aggregated data about provinces"}),
                ]),
            },
        ],
        output_fields: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_returns_correct_command_name() {
        let cmd = schema();
        assert_eq!(cmd.name, "census");
        assert_eq!(cmd.response_shape, "single-nested");
        let richness = cmd.args.iter().find(|a| a.name == "--richness").unwrap();
        assert_eq!(richness.min, Some(1));
        assert_eq!(richness.max, Some(3));
    }
}
