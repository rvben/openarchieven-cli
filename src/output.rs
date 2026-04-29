//! Response shaping and rendering.
//!
//! Three normalized shapes — `List`, `SingleFlat`, `SingleNested` —
//! covering the entire API surface. Each shape supports `json`, `table`,
//! and `markdown` rendering.

use crate::error::{Error, ErrorKind, Result};
use crate::tty::Format;
use serde_json::{Map, Value, json};
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    List,
    SingleFlat,
    SingleNested,
}

impl Shape {
    pub fn as_str(self) -> &'static str {
        match self {
            Shape::List => "list",
            Shape::SingleFlat => "single-flat",
            Shape::SingleNested => "single-nested",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Renderable {
    pub shape: Shape,
    pub body: Value,
    pub paginated: bool,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub total: Option<u64>,
}

impl Renderable {
    pub fn list(items: Value, paginated: bool, limit: Option<u32>, offset: Option<u32>) -> Self {
        Self {
            shape: Shape::List,
            body: items,
            paginated,
            limit,
            offset,
            total: None,
        }
    }

    pub fn single_flat(value: Value) -> Self {
        Self {
            shape: Shape::SingleFlat,
            body: value,
            paginated: false,
            limit: None,
            offset: None,
            total: None,
        }
    }

    pub fn single_nested(value: Value) -> Self {
        Self {
            shape: Shape::SingleNested,
            body: value,
            paginated: false,
            limit: None,
            offset: None,
            total: None,
        }
    }

    pub fn with_total(mut self, total: Option<u64>) -> Self {
        self.total = total;
        self
    }

    /// For `List`: build the wrapper `{items, total, limit, offset, paginated}`.
    pub fn list_envelope(&self, total_override: Option<u64>) -> Value {
        assert_eq!(
            self.shape,
            Shape::List,
            "list_envelope called on {:?}",
            self.shape
        );
        let arr = self.body.as_array().cloned().unwrap_or_default();
        let total = total_override
            .map(|n| json!(n))
            .unwrap_or_else(|| json!(arr.len() as u64));
        json!({
            "items": arr,
            "total": total,
            "limit": self.limit,
            "offset": self.offset,
            "paginated": self.paginated,
        })
    }
}

/// Apply a `--fields` filter using the union of observed object keys as the
/// known-fields set. Single-nested responses are rejected.
///
/// Use this from the dispatch layer; commands shouldn't need to publish a
/// row-level field list separately from the response itself.
pub fn apply_fields_auto(r: Renderable, fields: &[String]) -> Result<Renderable> {
    if r.shape == Shape::SingleNested {
        return Err(Error::new(
            ErrorKind::Validation,
            "--fields is not supported for nested single-record responses (try -o json | jq)",
        ));
    }
    let known_set: std::collections::BTreeSet<String> = match (r.shape, &r.body) {
        (Shape::List, Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_object())
            .flat_map(|o| o.keys().cloned())
            .collect(),
        (Shape::SingleFlat, Value::Object(o)) => o.keys().cloned().collect(),
        _ => return Ok(r),
    };
    if known_set.is_empty() {
        // Nothing to validate against (e.g. empty list); filter is a no-op.
        return Ok(r);
    }
    let known: Vec<&str> = known_set.iter().map(String::as_str).collect();
    apply_fields(r, fields, &known)
}

/// Apply a `--fields` filter to a `List` or `SingleFlat` `Renderable`.
///
/// Returns a new `Renderable`. Returns `validation` if any field is unknown,
/// or if the shape is `SingleNested`.
pub fn apply_fields(r: Renderable, fields: &[String], known_fields: &[&str]) -> Result<Renderable> {
    if r.shape == Shape::SingleNested {
        return Err(Error::new(
            ErrorKind::Validation,
            "--fields is not supported for nested single-record responses (try -o json | jq)",
        ));
    }
    let unknown: Vec<&str> = fields
        .iter()
        .filter(|f| !known_fields.contains(&f.as_str()))
        .map(String::as_str)
        .collect();
    if !unknown.is_empty() {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "unknown fields: {}. known: {}",
                unknown.join(","),
                known_fields.join(",")
            ),
        ));
    }
    let filter = |obj: &Map<String, Value>| -> Value {
        let mut out = Map::new();
        for f in fields {
            if let Some(v) = obj.get(f) {
                out.insert(f.clone(), v.clone());
            }
        }
        Value::Object(out)
    };
    let body = match (r.shape, &r.body) {
        (Shape::List, Value::Array(items)) => Value::Array(
            items
                .iter()
                .map(|v| match v {
                    Value::Object(o) => filter(o),
                    other => other.clone(),
                })
                .collect(),
        ),
        (Shape::SingleFlat, Value::Object(o)) => filter(o),
        _ => r.body.clone(),
    };
    Ok(Renderable { body, ..r })
}

/// Render the `Renderable` to `out` in the requested format.
pub fn render<W: Write>(
    out: &mut W,
    r: &Renderable,
    fmt: Format,
    pretty_json: bool,
) -> std::io::Result<()> {
    match fmt {
        Format::Json => render_json(out, r, pretty_json),
        Format::Table => render_table(out, r),
        Format::Markdown => render_markdown(out, r),
    }
}

fn render_json<W: Write>(out: &mut W, r: &Renderable, pretty: bool) -> std::io::Result<()> {
    match r.shape {
        Shape::List => {
            let envelope = r.list_envelope(r.total);
            if pretty {
                serde_json::to_writer_pretty(&mut *out, &envelope)?;
            } else {
                serde_json::to_writer(&mut *out, &envelope)?;
            }
        }
        _ => {
            if pretty {
                serde_json::to_writer_pretty(&mut *out, &r.body)?;
            } else {
                serde_json::to_writer(&mut *out, &r.body)?;
            }
        }
    }
    writeln!(out)
}

fn render_table<W: Write>(out: &mut W, r: &Renderable) -> std::io::Result<()> {
    use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    match r.shape {
        Shape::List => {
            let items = r.body.as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                writeln!(out, "(no results)")?;
                return Ok(());
            }
            let headers: Vec<String> = match items.first() {
                Some(Value::Object(o)) => o.keys().cloned().collect(),
                _ => vec!["value".into()],
            };
            t.set_header(headers.iter());
            for item in &items {
                let row: Vec<String> = match item {
                    Value::Object(o) => headers
                        .iter()
                        .map(|h| match o.get(h).cloned().unwrap_or(Value::Null) {
                            Value::String(s) => truncate(&s, 80),
                            other => truncate(&other.to_string(), 80),
                        })
                        .collect(),
                    Value::Null => vec!["".into()],
                    other => vec![truncate(&other.to_string(), 80)],
                };
                t.add_row(row);
            }
        }
        Shape::SingleFlat | Shape::SingleNested => {
            t.set_header(["key", "value"]);
            if let Value::Object(o) = &r.body {
                for (k, v) in o.iter() {
                    let s = match v {
                        Value::String(s) => truncate(s, 80),
                        Value::Object(_) | Value::Array(_) => {
                            truncate(&serde_json::to_string(v).unwrap_or_default(), 80)
                        }
                        other => other.to_string(),
                    };
                    t.add_row([k.as_str(), &s]);
                }
            }
        }
    }
    writeln!(out, "{t}")
}

fn render_markdown<W: Write>(out: &mut W, r: &Renderable) -> std::io::Result<()> {
    match r.shape {
        Shape::List => {
            let items = r.body.as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                writeln!(out, "_(no results)_")?;
                return Ok(());
            }
            let headers: Vec<String> = match items.first() {
                Some(Value::Object(o)) => o.keys().cloned().collect(),
                _ => vec!["value".into()],
            };
            writeln!(out, "| {} |", headers.join(" | "))?;
            writeln!(
                out,
                "| {} |",
                headers
                    .iter()
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | ")
            )?;
            for item in &items {
                let cells: Vec<String> = match item {
                    Value::Object(o) => headers
                        .iter()
                        .map(|h| md_cell(o.get(h).cloned().unwrap_or(Value::Null)))
                        .collect(),
                    other => vec![md_cell(other.clone())],
                };
                writeln!(out, "| {} |", cells.join(" | "))?;
            }
        }
        Shape::SingleFlat => {
            if let Value::Object(o) = &r.body {
                for (k, v) in o.iter() {
                    writeln!(out, "- **{k}**: {}", md_cell(v.clone()))?;
                }
            }
        }
        Shape::SingleNested => {
            if let Value::Object(o) = &r.body {
                for (k, v) in o.iter() {
                    match v {
                        Value::Object(_) | Value::Array(_) => {
                            writeln!(out, "- **{k}**:")?;
                            writeln!(out, "```json")?;
                            serde_json::to_writer_pretty(&mut *out, v)?;
                            writeln!(out)?;
                            writeln!(out, "```")?;
                        }
                        _ => writeln!(out, "- **{k}**: {}", md_cell(v.clone()))?,
                    }
                }
            }
        }
    }
    Ok(())
}

fn md_cell(v: Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.replace('|', "\\|").replace('\n', " "),
        Value::Object(_) | Value::Array(_) => {
            let s = serde_json::to_string(&v).unwrap_or_default();
            truncate(&s.replace('|', "\\|"), 200)
        }
        other => other.to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn list_envelope_includes_pagination_metadata() {
        let r = Renderable::list(json!([{"a": 1}, {"a": 2}]), true, Some(10), Some(0));
        let v = r.list_envelope(Some(123));
        assert_eq!(v["total"], 123);
        assert_eq!(v["limit"], 10);
        assert_eq!(v["offset"], 0);
        assert_eq!(v["paginated"], true);
        assert_eq!(v["items"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn list_envelope_falls_back_to_len_when_no_total() {
        let r = Renderable::list(json!([{"a": 1}]), false, None, None);
        let v = r.list_envelope(None);
        assert_eq!(v["total"], 1);
        assert_eq!(v["paginated"], false);
        assert!(v["limit"].is_null());
    }

    #[test]
    fn fields_filter_keeps_only_named_keys() {
        let r = Renderable::list(json!([{"a":1,"b":2,"c":3}]), false, None, None);
        let r = apply_fields(r, &["a".into(), "c".into()], &["a", "b", "c"]).unwrap();
        let items = r.body.as_array().unwrap();
        let item = items[0].as_object().unwrap();
        assert!(item.contains_key("a"));
        assert!(!item.contains_key("b"));
        assert!(item.contains_key("c"));
    }

    #[test]
    fn fields_filter_rejects_unknown() {
        let r = Renderable::list(json!([{"a":1}]), false, None, None);
        let err = apply_fields(r, &["zzz".into()], &["a"]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
        assert!(err.message.contains("zzz"));
    }

    #[test]
    fn fields_filter_rejects_nested_shape() {
        let r = Renderable::single_nested(json!({"a": {"b": 1}}));
        let err = apply_fields(r, &["a".into()], &["a"]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
        assert!(err.message.contains("nested"));
    }

    #[test]
    fn fields_auto_derives_known_set_from_list_items() {
        let r = Renderable::list(
            json!([{"a": 1, "b": 2}, {"a": 3, "c": 4}]),
            false,
            None,
            None,
        );
        let r = apply_fields_auto(r, &["a".into()]).unwrap();
        let items = r.body.as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].as_object().unwrap().contains_key("a"));
        assert!(!items[0].as_object().unwrap().contains_key("b"));
    }

    #[test]
    fn fields_auto_rejects_unknown_against_observed_keys() {
        let r = Renderable::list(json!([{"a": 1}]), false, None, None);
        let err = apply_fields_auto(r, &["zzz".into()]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
        assert!(err.message.contains("zzz"));
    }

    #[test]
    fn fields_auto_filters_single_flat() {
        let r = Renderable::single_flat(json!({"x": 1, "y": 2}));
        let r = apply_fields_auto(r, &["x".into()]).unwrap();
        let obj = r.body.as_object().unwrap();
        assert!(obj.contains_key("x"));
        assert!(!obj.contains_key("y"));
    }

    #[test]
    fn fields_auto_rejects_single_nested_shape() {
        let r = Renderable::single_nested(json!({"a": {"b": 1}}));
        let err = apply_fields_auto(r, &["a".into()]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
        assert!(err.message.contains("nested"));
    }

    #[test]
    fn fields_auto_is_noop_for_empty_list() {
        let r = Renderable::list(json!([]), false, None, None);
        let r = apply_fields_auto(r, &["anything".into()]).unwrap();
        assert_eq!(r.body.as_array().unwrap().len(), 0);
    }

    #[test]
    fn json_render_list_writes_envelope() {
        let r = Renderable::list(json!([{"x": 1}]), true, Some(5), Some(0));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Json, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let v: Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["items"][0]["x"], 1);
        assert_eq!(v["paginated"], true);
    }

    #[test]
    fn json_render_single_writes_object_directly() {
        let r = Renderable::single_flat(json!({"k": "v"}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Json, false).unwrap();
        let v: Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["k"], "v");
        assert!(v.get("items").is_none());
    }

    #[test]
    fn markdown_renders_list_table() {
        let r = Renderable::list(json!([{"a": 1, "b": "x"}]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("| a | b |"));
        assert!(s.contains("| --- | --- |"));
    }

    #[test]
    fn markdown_escapes_pipes_in_strings() {
        let r = Renderable::list(json!([{"name": "a|b"}]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("a\\|b"));
    }

    #[test]
    fn markdown_renders_nested_as_fenced_json() {
        let r = Renderable::single_nested(json!({"name": "alice", "addr": {"city": "AMS"}}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("- **name**: alice"));
        assert!(s.contains("- **addr**:"));
        assert!(s.contains("```json"));
    }
}
