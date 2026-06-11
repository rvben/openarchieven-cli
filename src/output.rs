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

/// Projection tree built from `--fields` paths.
///
/// `--fields docs,total` produces `{docs: Leaf, total: Leaf}`.
/// `--fields docs.year,docs.archive_code` produces `{docs: {year: Leaf, archive_code: Leaf}}`.
/// Mixing the two — `--fields docs,docs.year` — collapses to `Leaf` (the more permissive choice
/// wins; once the user says "keep all of `docs`", a narrower sub-path is redundant).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Projection {
    Leaf,
    Tree(std::collections::BTreeMap<String, Projection>),
}

impl Projection {
    fn from_paths(paths: &[String]) -> Self {
        let mut root = Projection::Tree(std::collections::BTreeMap::new());
        for path in paths {
            let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
            if segments.is_empty() {
                continue;
            }
            insert_path(&mut root, &segments);
        }
        root
    }

    fn first_segments(&self) -> Vec<&str> {
        match self {
            Projection::Leaf => Vec::new(),
            Projection::Tree(t) => t.keys().map(String::as_str).collect(),
        }
    }
}

fn insert_path(node: &mut Projection, segments: &[&str]) {
    match node {
        Projection::Leaf => {} // Already permissive; nothing to add.
        Projection::Tree(children) => {
            let head = segments[0].to_string();
            let rest = &segments[1..];
            if rest.is_empty() {
                children.insert(head, Projection::Leaf);
            } else {
                let entry = children
                    .entry(head)
                    .or_insert_with(|| Projection::Tree(std::collections::BTreeMap::new()));
                insert_path(entry, rest);
            }
        }
    }
}

/// Project a JSON value through a `Projection` tree.
///
/// Object: keep only keys present in the tree, recursing into children.
/// Array: recursively project each element with the *same* projection (arrays-of-objects
/// is the common case — every doc gets the same field subset).
/// Scalar: returned unchanged when reached at a `Leaf`; the tree should never bottom out
/// on a scalar (validation rejects unknown paths), but if it does we return as-is.
fn project(value: &Value, proj: &Projection) -> Value {
    match (proj, value) {
        (Projection::Leaf, _) => value.clone(),
        (Projection::Tree(tree), Value::Object(o)) => {
            let mut out = Map::new();
            for (k, sub) in tree {
                if let Some(v) = o.get(k) {
                    out.insert(k.clone(), project(v, sub));
                }
            }
            Value::Object(out)
        }
        (Projection::Tree(_), Value::Array(items)) => {
            Value::Array(items.iter().map(|v| project(v, proj)).collect())
        }
        (Projection::Tree(_), _) => value.clone(),
    }
}

/// Apply a `--fields` filter using the union of observed top-level object keys as the
/// known-fields set. Single-nested responses are now supported — they project just like
/// `SingleFlat`, but only the user's explicit dot-paths are kept.
///
/// Use this from the dispatch layer; commands shouldn't need to publish a
/// row-level field list separately from the response itself.
pub fn apply_fields_auto(r: Renderable, fields: &[String]) -> Result<Renderable> {
    let known_set: std::collections::BTreeSet<String> = match (r.shape, &r.body) {
        (Shape::List, Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_object())
            .flat_map(|o| o.keys().cloned())
            .collect(),
        (Shape::SingleFlat | Shape::SingleNested, Value::Object(o)) => o.keys().cloned().collect(),
        _ => return Ok(r),
    };
    if known_set.is_empty() {
        // Nothing to validate against (e.g. empty list); filter is a no-op.
        return Ok(r);
    }
    let known: Vec<&str> = known_set.iter().map(String::as_str).collect();
    apply_fields(r, fields, &known)
}

/// Apply a `--fields` filter to any `Renderable`.
///
/// Fields may be dot-paths (`docs.eventdate.year`). Only the *first* segment is validated
/// against `known_fields`; deeper segments are trusted (the response shape is unbounded
/// and we can't enforce a closed schema below the top level).
pub fn apply_fields(r: Renderable, fields: &[String], known_fields: &[&str]) -> Result<Renderable> {
    let projection = Projection::from_paths(fields);
    let unknown: Vec<&str> = projection
        .first_segments()
        .into_iter()
        .filter(|seg| !known_fields.contains(seg))
        .collect();
    if !unknown.is_empty() {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "unknown fields: {}. known top-level: {}",
                unknown.join(","),
                known_fields.join(",")
            ),
        ));
    }
    let body = match (r.shape, &r.body) {
        (Shape::List, Value::Array(_)) => project(&r.body, &projection),
        (Shape::SingleFlat | Shape::SingleNested, Value::Object(_)) => {
            project(&r.body, &projection)
        }
        _ => r.body.clone(),
    };
    Ok(Renderable { body, ..r })
}

/// Validate that the chosen output format is compatible with the response shape.
///
/// `ndjson` requires a list-shaped response — single records cannot be streamed
/// line-by-line in a way that's meaningfully different from compact JSON.
pub fn ensure_format_compatible(r: &Renderable, fmt: Format) -> Result<()> {
    if fmt == Format::Ndjson && r.shape != Shape::List {
        return Err(Error::new(
            ErrorKind::Validation,
            format!(
                "--output ndjson requires a list response; this command returns {}",
                r.shape.as_str()
            ),
        ));
    }
    Ok(())
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
        Format::Ndjson => render_ndjson(out, r),
        Format::Table | Format::Text => render_table(out, r),
        Format::Markdown => render_markdown(out, r),
    }
}

fn render_ndjson<W: Write>(out: &mut W, r: &Renderable) -> std::io::Result<()> {
    debug_assert_eq!(
        r.shape,
        Shape::List,
        "ndjson called on non-list shape (callers must pre-validate)"
    );
    if let Value::Array(items) = &r.body {
        for item in items {
            serde_json::to_writer(&mut *out, item)?;
            writeln!(out)?;
        }
    }
    Ok(())
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
                        .map(|h| {
                            match humanise_value(h, o.get(h).cloned().unwrap_or(Value::Null)) {
                                Value::String(s) => truncate(&s, 80),
                                other => truncate(&other.to_string(), 80),
                            }
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
                    let hv = humanise_value(k, v.clone());
                    let s = match &hv {
                        Value::String(s) => truncate(s, 80),
                        Value::Object(_) | Value::Array(_) => {
                            truncate(&serde_json::to_string(&hv).unwrap_or_default(), 80)
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
                        .map(|h| {
                            md_cell(humanise_value(h, o.get(h).cloned().unwrap_or(Value::Null)))
                        })
                        .collect(),
                    other => vec![md_cell(other.clone())],
                };
                writeln!(out, "| {} |", cells.join(" | "))?;
            }
        }
        Shape::SingleFlat => {
            if let Value::Object(o) = &r.body {
                for (k, v) in o.iter() {
                    writeln!(out, "- **{k}**: {}", md_cell(humanise_value(k, v.clone())))?;
                }
            }
        }
        Shape::SingleNested => {
            if let Value::Object(o) = &r.body {
                for (k, v) in o.iter() {
                    let hv = humanise_value(k, v.clone());
                    match hv {
                        Value::Object(_) | Value::Array(_) => {
                            writeln!(out, "- **{k}**:")?;
                            writeln!(out, "```json")?;
                            serde_json::to_writer_pretty(&mut *out, &hv)?;
                            writeln!(out)?;
                            writeln!(out, "```")?;
                        }
                        _ => writeln!(out, "- **{k}**: {}", md_cell(hv))?,
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

fn humanise_value(field: &str, v: Value) -> Value {
    match (field, v) {
        ("eventdate", Value::Object(o))
        | ("birthdate", Value::Object(o))
        | ("deathdate", Value::Object(o)) => {
            let d = o.get("day").and_then(Value::as_i64);
            let m = o.get("month").and_then(Value::as_i64);
            let y = o.get("year").and_then(Value::as_i64);
            if let (Some(d), Some(m), Some(y)) = (d, m, y) {
                Value::String(format!("{d:02}-{m:02}-{y:04}"))
            } else {
                Value::Object(o)
            }
        }
        ("personname", Value::String(s)) => {
            let trimmed = s.trim_start_matches('#').trim_start();
            Value::String(trimmed.to_string())
        }
        (_, v) => v,
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

    // ── Shape::as_str ──────────────────────────────────────────────────────────

    #[test]
    fn shape_as_str_returns_stable_strings() {
        assert_eq!(Shape::List.as_str(), "list");
        assert_eq!(Shape::SingleFlat.as_str(), "single-flat");
        assert_eq!(Shape::SingleNested.as_str(), "single-nested");
    }

    // ── Renderable constructors / with_total ──────────────────────────────────

    #[test]
    fn with_total_sets_total_field() {
        let r = Renderable::list(json!([]), false, None, None).with_total(Some(42));
        assert_eq!(r.total, Some(42));
    }

    #[test]
    fn with_total_none_leaves_total_as_none() {
        let r = Renderable::list(json!([]), false, None, None).with_total(None);
        assert_eq!(r.total, None);
    }

    // ── list_envelope ─────────────────────────────────────────────────────────

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

    // ── apply_fields ──────────────────────────────────────────────────────────

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
    fn fields_filter_projects_single_nested_top_level() {
        let r = Renderable::single_nested(json!({"a": {"b": 1}, "c": 9}));
        let r = apply_fields(r, &["a".into()], &["a", "c"]).unwrap();
        let obj = r.body.as_object().unwrap();
        assert!(obj.contains_key("a"));
        assert!(!obj.contains_key("c"));
        assert_eq!(obj["a"], json!({"b": 1}));
    }

    #[test]
    fn fields_filter_projects_dot_path_into_nested_object() {
        let r = Renderable::single_nested(json!({"a": {"b": 1, "c": 2}, "d": 9}));
        let r = apply_fields(r, &["a.b".into()], &["a", "d"]).unwrap();
        let obj = r.body.as_object().unwrap();
        assert_eq!(obj["a"], json!({"b": 1}));
        assert!(!obj.contains_key("d"));
    }

    #[test]
    fn fields_filter_projects_dot_path_into_array_of_objects() {
        // List body with dot-path: each item is projected to {year, archive_code}.
        let r = Renderable::list(
            json!([
                {"archive_code": "a", "personname": "Alice", "eventdate": {"year": 1900, "month": 1}},
                {"archive_code": "b", "personname": "Bob",   "eventdate": {"year": 1950, "month": 2}}
            ]),
            false,
            None,
            None,
        );
        let r = apply_fields(
            r,
            &["archive_code".into(), "eventdate.year".into()],
            &["archive_code", "personname", "eventdate"],
        )
        .unwrap();
        let items = r.body.as_array().unwrap();
        assert_eq!(
            items[0],
            json!({"archive_code": "a", "eventdate": {"year": 1900}})
        );
        assert_eq!(
            items[1],
            json!({"archive_code": "b", "eventdate": {"year": 1950}})
        );
    }

    #[test]
    fn fields_filter_collapses_explicit_subpath_under_explicit_leaf() {
        // --fields docs,docs.year ⇒ the broader docs wins (Leaf).
        let r = Renderable::list(
            json!([{"docs": {"year": 1900, "place": "NL"}, "extra": 1}]),
            false,
            None,
            None,
        );
        let r = apply_fields(r, &["docs".into(), "docs.year".into()], &["docs", "extra"]).unwrap();
        let items = r.body.as_array().unwrap();
        // docs is kept whole; extra is dropped.
        assert_eq!(items[0], json!({"docs": {"year": 1900, "place": "NL"}}));
    }

    #[test]
    fn fields_filter_validation_only_checks_first_segment() {
        // Unknown deep segments are not rejected — the response shape below the top level
        // is open. Only the first segment must be observed.
        let r = Renderable::list(json!([{"a": {"b": 1}}]), false, None, None);
        let r = apply_fields(r, &["a.zzz".into()], &["a"]).unwrap();
        // `a` is kept but is empty (b not in projection).
        let items = r.body.as_array().unwrap();
        assert_eq!(items[0], json!({"a": {}}));
    }

    #[test]
    fn fields_filter_on_single_flat_keeps_named_keys() {
        let r = Renderable::single_flat(json!({"a": 1, "b": 2}));
        let r = apply_fields(r, &["a".into()], &["a", "b"]).unwrap();
        let obj = r.body.as_object().unwrap();
        assert!(obj.contains_key("a"));
        assert!(!obj.contains_key("b"));
    }

    #[test]
    fn fields_filter_list_non_object_items_pass_through_unchanged() {
        // When list items are not objects the filter just clones them.
        let r = Renderable::list(json!(["hello", "world"]), false, None, None);
        let r = apply_fields(r, &[], &["anything"]).unwrap();
        let items = r.body.as_array().unwrap();
        assert_eq!(items[0], json!("hello"));
    }

    #[test]
    fn fields_filter_passthrough_for_non_array_non_object_body() {
        // A list whose body is not an array falls into the `_ => r.body.clone()` arm.
        let r = Renderable {
            shape: Shape::List,
            body: json!(42),
            paginated: false,
            limit: None,
            offset: None,
            total: None,
        };
        let r = apply_fields(r, &[], &["x"]).unwrap();
        assert_eq!(r.body, json!(42));
    }

    // ── apply_fields_auto ─────────────────────────────────────────────────────

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
    fn fields_auto_projects_single_nested_dot_paths() {
        let r = Renderable::single_nested(json!({"a": {"b": 1, "c": 2}, "d": 9}));
        let r = apply_fields_auto(r, &["a.b".into()]).unwrap();
        assert_eq!(r.body, json!({"a": {"b": 1}}));
    }

    #[test]
    fn fields_auto_rejects_unknown_first_segment_on_nested() {
        let r = Renderable::single_nested(json!({"a": {"b": 1}}));
        let err = apply_fields_auto(r, &["zzz.b".into()]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
        assert!(err.message.contains("zzz"));
    }

    #[test]
    fn fields_auto_is_noop_for_empty_list() {
        let r = Renderable::list(json!([]), false, None, None);
        let r = apply_fields_auto(r, &["anything".into()]).unwrap();
        assert_eq!(r.body.as_array().unwrap().len(), 0);
    }

    #[test]
    fn fields_auto_passthrough_for_non_array_list_body() {
        // Body that is not an array falls into `_ => return Ok(r)`.
        let r = Renderable {
            shape: Shape::List,
            body: json!(null),
            paginated: false,
            limit: None,
            offset: None,
            total: None,
        };
        let r = apply_fields_auto(r, &["x".into()]).unwrap();
        assert_eq!(r.body, json!(null));
    }

    // ── ensure_format_compatible ──────────────────────────────────────────────

    #[test]
    fn ensure_format_compatible_allows_ndjson_on_list() {
        let r = Renderable::list(json!([{"x": 1}]), false, None, None);
        assert!(ensure_format_compatible(&r, Format::Ndjson).is_ok());
    }

    #[test]
    fn ensure_format_compatible_rejects_ndjson_on_single_flat() {
        let r = Renderable::single_flat(json!({"x": 1}));
        let err = ensure_format_compatible(&r, Format::Ndjson).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
        assert!(err.message.contains("ndjson"));
        assert!(err.message.contains("single-flat"));
    }

    #[test]
    fn ensure_format_compatible_rejects_ndjson_on_single_nested() {
        let r = Renderable::single_nested(json!({"a": {"b": 1}}));
        let err = ensure_format_compatible(&r, Format::Ndjson).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
        assert!(err.message.contains("single-nested"));
    }

    #[test]
    fn ensure_format_compatible_allows_other_formats_on_any_shape() {
        let single = Renderable::single_flat(json!({"x": 1}));
        for fmt in [Format::Json, Format::Table, Format::Markdown] {
            assert!(ensure_format_compatible(&single, fmt).is_ok());
        }
    }

    // ── ndjson renderer ───────────────────────────────────────────────────────

    #[test]
    fn ndjson_render_list_emits_one_doc_per_line() {
        let r = Renderable::list(
            json!([{"a": 1}, {"a": 2}, {"a": 3}]),
            true,
            Some(50),
            Some(0),
        );
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Ndjson, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(
            serde_json::from_str::<Value>(lines[0]).unwrap(),
            json!({"a": 1})
        );
        assert_eq!(
            serde_json::from_str::<Value>(lines[1]).unwrap(),
            json!({"a": 2})
        );
        assert_eq!(
            serde_json::from_str::<Value>(lines[2]).unwrap(),
            json!({"a": 3})
        );
        // No envelope, no surrounding metadata.
        assert!(!s.contains("items"));
        assert!(!s.contains("total"));
        assert!(!s.contains("paginated"));
    }

    #[test]
    fn ndjson_render_empty_list_emits_no_output() {
        let r = Renderable::list(json!([]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Ndjson, false).unwrap();
        assert!(
            buf.is_empty(),
            "ndjson should emit nothing for empty list, got: {buf:?}"
        );
    }

    #[test]
    fn ndjson_each_line_is_compact() {
        let r = Renderable::list(json!([{"a": 1, "b": "x"}]), false, None, None);
        let mut buf = Vec::new();
        // Pretty flag is ignored for ndjson — each line must be compact.
        render(&mut buf, &r, Format::Ndjson, true).unwrap();
        let s = String::from_utf8(buf).unwrap();
        // No indentation, no internal newlines on the data line.
        assert_eq!(s.trim_end(), r#"{"a":1,"b":"x"}"#);
    }

    // ── JSON renderer ─────────────────────────────────────────────────────────

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
    fn json_render_list_pretty() {
        let r = Renderable::list(json!([{"x": 1}]), false, None, None);
        let mut buf_pretty = Vec::new();
        render(&mut buf_pretty, &r, Format::Json, true).unwrap();
        let pretty = String::from_utf8(buf_pretty).unwrap();
        let mut buf_compact = Vec::new();
        render(&mut buf_compact, &r, Format::Json, false).unwrap();
        let compact = String::from_utf8(buf_compact).unwrap();

        assert!(pretty.contains("\n  \"items\": ["));
        assert!(pretty.contains("\n      \"x\": 1"));
        assert!(!compact.contains("\n  "));
        assert!(pretty.len() > compact.len());

        let v: Value = serde_json::from_str(pretty.trim()).unwrap();
        assert_eq!(v["items"][0]["x"], 1);
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
    fn json_render_single_nested_pretty() {
        let r = Renderable::single_nested(json!({"a": {"b": 2}}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Json, true).unwrap();
        let s = String::from_utf8(buf).unwrap();
        // Outer object indent = 2, nested object indent = 4.
        assert_eq!(s.trim_end(), "{\n  \"a\": {\n    \"b\": 2\n  }\n}");
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn json_render_single_flat_pretty() {
        let r = Renderable::single_flat(json!({"foo": "bar"}));
        let mut buf_pretty = Vec::new();
        render(&mut buf_pretty, &r, Format::Json, true).unwrap();
        let pretty = String::from_utf8(buf_pretty).unwrap();
        let mut buf_compact = Vec::new();
        render(&mut buf_compact, &r, Format::Json, false).unwrap();
        let compact = String::from_utf8(buf_compact).unwrap();

        assert_eq!(pretty.trim_end(), "{\n  \"foo\": \"bar\"\n}");
        assert_eq!(compact.trim_end(), "{\"foo\":\"bar\"}");
    }

    // ── Table renderer ────────────────────────────────────────────────────────

    #[test]
    fn table_render_empty_list_prints_no_results() {
        let r = Renderable::list(json!([]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("(no results)"));
    }

    #[test]
    fn table_render_list_with_objects() {
        let r = Renderable::list(json!([{"name": "alice", "age": 30}]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("name"));
        assert!(s.contains("alice"));
        assert!(s.contains("age"));
        assert!(s.contains("30"));
    }

    #[test]
    fn table_render_list_null_item_shows_empty_row() {
        let r = Renderable::list(json!([null, null]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let rows: Vec<&str> = s.lines().filter(|l| l.starts_with('│')).collect();
        // Header row + two empty data rows.
        assert_eq!(rows.len(), 3);
        assert!(rows[0].contains("value"));
        for row in &rows[1..] {
            let cell = row.trim_matches('│').trim();
            assert!(cell.is_empty(), "expected empty cell, got {row:?}");
        }
    }

    #[test]
    fn table_render_list_scalar_item() {
        let r = Renderable::list(json!([42, 99]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let data_rows: Vec<&str> = s
            .lines()
            .filter(|l| l.starts_with('│') && !l.contains("value"))
            .collect();
        assert_eq!(data_rows.len(), 2);
        assert!(data_rows[0].contains("42"));
        assert!(data_rows[1].contains("99"));
    }

    #[test]
    fn table_render_list_object_with_nested_value_serialized() {
        let r = Renderable::list(json!([{"meta": {"x": 1}}]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        // Nested object must be serialized as compact JSON, not Debug-formatted.
        assert!(
            s.contains(r#"{"x":1}"#),
            "expected serialized JSON in cell, got:\n{s}"
        );
    }

    #[test]
    fn table_render_list_missing_key_renders_null() {
        // The renderer fills missing keys with `Value::Null`, whose `to_string()` is "null".
        let r = Renderable::list(json!([{"a": 1, "b": 2}, {"a": 3}]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        // The second row has only `a=3`; `b` is rendered as the literal "null".
        let second_row = s
            .lines()
            .find(|l| l.starts_with('│') && l.contains(" 3 "))
            .expect("second data row present");
        assert!(
            second_row.contains("null"),
            "expected 'null' in row, got: {second_row:?}"
        );
    }

    #[test]
    fn table_render_single_flat() {
        let r = Renderable::single_flat(json!({"city": "Amsterdam", "pop": 900000}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("key"));
        assert!(s.contains("value"));
        assert!(s.contains("city"));
        assert!(s.contains("Amsterdam"));
    }

    #[test]
    fn table_render_single_nested_with_object_value_serialized() {
        let r = Renderable::single_nested(json!({"addr": {"city": "AMS"}}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        // Nested object rendered as compact JSON in the value column.
        assert!(s.contains(r#"{"city":"AMS"}"#), "got:\n{s}");
    }

    #[test]
    fn table_render_single_nested_with_array_value_serialized() {
        let r = Renderable::single_nested(json!({"tags": ["a", "b"]}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains(r#"["a","b"]"#), "got:\n{s}");
    }

    #[test]
    fn table_render_single_flat_scalar_values() {
        let r = Renderable::single_flat(json!({"count": 42, "active": true}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Table, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("42"));
        assert!(s.contains("true"));
    }

    // ── Markdown renderer ─────────────────────────────────────────────────────

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
    fn markdown_renders_empty_list() {
        let r = Renderable::list(json!([]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("_(no results)_"));
    }

    #[test]
    fn markdown_renders_list_non_object_items() {
        let r = Renderable::list(json!(["hello", "world"]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("| value |"));
        assert!(s.contains("hello"));
    }

    #[test]
    fn markdown_renders_list_null_cells_as_empty() {
        let r = Renderable::list(json!([{"a": 1, "b": null}]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[0], "| a | b |");
        assert_eq!(lines[1], "| --- | --- |");
        assert_eq!(lines[2], "| 1 |  |");
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
    fn markdown_replaces_newlines_in_cells() {
        let r = Renderable::list(json!([{"text": "line1\nline2"}]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("line1 line2"));
    }

    #[test]
    fn markdown_renders_single_flat() {
        let r = Renderable::single_flat(json!({"name": "alice", "age": 30}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("- **name**: alice"));
        assert!(s.contains("- **age**: 30"));
    }

    #[test]
    fn markdown_renders_single_flat_null_value_as_empty() {
        let r = Renderable::single_flat(json!({"key": null}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("- **key**: "));
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

    #[test]
    fn markdown_renders_nested_array_as_fenced_json() {
        let r = Renderable::single_nested(json!({"tags": ["a", "b"]}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("- **tags**:"));
        assert!(s.contains("```json"));
        assert!(s.contains("\"a\""));
    }

    #[test]
    fn markdown_renders_nested_scalar() {
        let r = Renderable::single_nested(json!({"count": 7}));
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("- **count**: 7"));
    }

    #[test]
    fn markdown_object_cell_escapes_pipes_in_serialized_json() {
        let r = Renderable::list(json!([{"meta": {"key": "a|b"}}]), false, None, None);
        let mut buf = Vec::new();
        render(&mut buf, &r, Format::Markdown, false).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        // Header, separator, single data row.
        assert_eq!(lines[0], "| meta |");
        assert_eq!(lines[1], "| --- |");
        // The serialized object cell has the inner `|` escaped as `\|`.
        assert_eq!(lines[2], r#"| {"key":"a\|b"} |"#);
    }

    // ── truncate ──────────────────────────────────────────────────────────────

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string_appends_ellipsis() {
        let result = truncate("abcdef", 5);
        assert!(result.ends_with('…'));
        assert_eq!(result.chars().count(), 5);
    }

    #[test]
    fn truncate_multibyte_chars_counted_correctly() {
        // Unicode characters should be counted by char not byte.
        let s: String = "αβγδε".to_string(); // 5 chars, 10 bytes
        assert_eq!(truncate(&s, 5), "αβγδε");
        let result = truncate(&s, 4);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn table_renders_eventdate_as_iso_dmy() {
        let r = Renderable::list(
            json!([{"eventdate": {"day": 31, "month": 5, "year": 1895}, "personname": "# Jansen"}]),
            false,
            None,
            None,
        );
        let mut buf = Vec::new();
        render_table(&mut buf, &r).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("31-05-1895"), "table:\n{s}");
        assert!(!s.contains("\"day\""), "raw json leaked: {s}");
        assert!(s.contains(" Jansen"), "personname not stripped: {s}");
        assert!(!s.contains("# Jansen"), "personname header leaked: {s}");
    }

    #[test]
    fn markdown_strips_personname_heading() {
        let r = Renderable::list(json!([{"personname": "# Jansen-Walet"}]), false, None, None);
        let mut buf = Vec::new();
        render_markdown(&mut buf, &r).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(!s.contains("| # Jansen"), "markdown:\n{s}");
        assert!(s.contains("| Jansen-Walet |"), "markdown:\n{s}");
    }

    #[test]
    fn table_renders_single_nested_eventdate_as_iso_dmy() {
        let r =
            Renderable::single_nested(json!({"eventdate": {"day": 7, "month": 8, "year": 1923}}));
        let mut buf = Vec::new();
        render_table(&mut buf, &r).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("07-08-1923"), "table:\n{s}");
        assert!(!s.contains("\"day\""), "raw json leaked: {s}");
    }

    #[test]
    fn table_renders_single_flat_eventdate_as_iso_dmy() {
        let r =
            Renderable::single_flat(json!({"eventdate": {"day": 15, "month": 3, "year": 1888}}));
        let mut buf = Vec::new();
        render_table(&mut buf, &r).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("15-03-1888"), "table:\n{s}");
        assert!(!s.contains("\"day\""), "raw json leaked: {s}");
    }
}
