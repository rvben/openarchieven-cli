//! On-disk JSON cache for upstream GET responses.
//!
//! Keys are sha256 of `(canonical_base_url, method, path, sorted_query, ttl_class)`.
//! Files are written atomically (`tempfile::persist`) and read without locking.
//! Destructive operations (`clear`, `prune`) acquire an exclusive `fs4` lock.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Derive a cache key.
///
/// `params` are sorted by name; multi-value params are not supported.
/// `ttl_class` is the empty string for fixed-TTL endpoints, or a value such
/// as `today` (e.g. `2026-04-29`) for `yearsago`.
pub fn key(
    base_url: &str,
    method: &str,
    path: &str,
    params: &BTreeMap<String, String>,
    ttl_class: &str,
) -> String {
    let canonical_base = canonical_base_url(base_url);
    let q = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let mut h = Sha256::new();
    h.update(canonical_base.as_bytes());
    h.update(b"\0");
    h.update(method.as_bytes());
    h.update(b"\0");
    h.update(path.as_bytes());
    h.update(b"\0");
    h.update(q.as_bytes());
    h.update(b"\0");
    h.update(ttl_class.as_bytes());
    let bytes = h.finalize();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn canonical_base_url(s: &str) -> String {
    // `url::Url::parse` normalizes the host to lowercase; use its serialization
    // as the canonical form so that mixed-case hosts compare equal.
    if let Ok(u) = url::Url::parse(s) {
        u.as_str().trim_end_matches('/').to_string()
    } else {
        s.trim_end_matches('/').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn key_changes_with_base_url() {
        let a = key("https://api.openarchieven.nl/1.1", "GET", "/x", &p(&[]), "");
        let b = key("https://example.test/1.1", "GET", "/x", &p(&[]), "");
        assert_ne!(a, b);
    }

    #[test]
    fn key_changes_with_query() {
        let a = key("https://example.com", "GET", "/x", &p(&[("a", "1")]), "");
        let b = key("https://example.com", "GET", "/x", &p(&[("a", "2")]), "");
        assert_ne!(a, b);
    }

    #[test]
    fn key_changes_with_ttl_class() {
        let a = key("https://example.com", "GET", "/x", &p(&[]), "2026-04-29");
        let b = key("https://example.com", "GET", "/x", &p(&[]), "2026-04-30");
        assert_ne!(a, b);
    }

    #[test]
    fn key_stable_across_param_insertion_order() {
        let a = key(
            "https://e.com",
            "GET",
            "/x",
            &p(&[("a", "1"), ("b", "2")]),
            "",
        );
        let b = key(
            "https://e.com",
            "GET",
            "/x",
            &p(&[("b", "2"), ("a", "1")]),
            "",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn key_normalizes_host_case() {
        let a = key("https://API.openarchieven.NL/1.1", "GET", "/x", &p(&[]), "");
        let b = key("https://api.openarchieven.nl/1.1", "GET", "/x", &p(&[]), "");
        assert_eq!(a, b);
    }

    #[test]
    fn key_normalizes_trailing_slash() {
        let a = key(
            "https://api.openarchieven.nl/1.1/",
            "GET",
            "/x",
            &p(&[]),
            "",
        );
        let b = key("https://api.openarchieven.nl/1.1", "GET", "/x", &p(&[]), "");
        assert_eq!(a, b);
    }

    #[test]
    fn key_is_64_hex_chars() {
        let k = key("https://e.com", "GET", "/x", &p(&[]), "");
        assert_eq!(k.len(), 64);
        assert!(k.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
