use chrono::{Duration as ChronoDuration, Utc};
use openarchieven::cache::{Cache, Entry};
use openarchieven::commands::cache_cmd;
use openarchieven::error::ErrorKind;
use openarchieven::output::Shape;
use serde_json::json;
use tempfile::tempdir;

fn fresh_entry(now: chrono::DateTime<Utc>) -> Entry {
    Entry {
        url: "https://example.com/x".into(),
        fetched_at: now,
        expires_at: Some(now + ChronoDuration::hours(1)),
        body: json!({"a": 1}),
    }
}

fn stale_entry(now: chrono::DateTime<Utc>) -> Entry {
    Entry {
        url: "https://example.com/y".into(),
        fetched_at: now - ChronoDuration::days(1),
        expires_at: Some(now - ChronoDuration::hours(1)),
        body: json!({"a": 2}),
    }
}

#[test]
fn info_reports_zero_entries_for_fresh_dir() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();

    let r = cache_cmd::info(&cache).unwrap();
    assert_eq!(r.shape, Shape::SingleFlat);
    assert_eq!(r.body["entries"], json!(0));
    assert_eq!(r.body["bytes"], json!(0));
    assert_eq!(r.body["oldest"], json!(null));
    assert_eq!(r.body["newest"], json!(null));
    let expected_root = std::fs::canonicalize(dir.path())
        .unwrap()
        .display()
        .to_string();
    assert_eq!(r.body["root"], json!(expected_root));
}

#[test]
fn info_counts_entries_and_reports_oldest_and_newest() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let now = Utc::now();
    cache.put(&"a".repeat(64), &fresh_entry(now));
    cache.put(&"b".repeat(64), &stale_entry(now));

    let r = cache_cmd::info(&cache).unwrap();
    assert_eq!(r.body["entries"], json!(2));
    assert!(r.body["bytes"].as_u64().unwrap() > 0);
    assert!(r.body["oldest"].is_string());
    assert!(r.body["newest"].is_string());
}

#[test]
fn clear_without_yes_is_validation_error() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let now = Utc::now();
    cache.put(&"a".repeat(64), &fresh_entry(now));

    let err = cache_cmd::clear(&cache, false).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.message().contains("--yes"));

    // Entry still present after refusal.
    assert_eq!(cache.info().unwrap().entries, 1);
}

#[test]
fn clear_with_yes_removes_all_entries() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let now = Utc::now();
    cache.put(&"a".repeat(64), &fresh_entry(now));
    cache.put(&"b".repeat(64), &stale_entry(now));

    let r = cache_cmd::clear(&cache, true).unwrap();
    assert_eq!(r.shape, Shape::SingleFlat);
    assert_eq!(r.body["deleted"], json!(2));
    assert_eq!(cache.info().unwrap().entries, 0);
}

#[test]
fn prune_drops_only_expired_entries() {
    let dir = tempdir().unwrap();
    let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
    let now = Utc::now();
    cache.put(&"a".repeat(64), &fresh_entry(now));
    cache.put(&"b".repeat(64), &stale_entry(now));

    let r = cache_cmd::prune(&cache).unwrap();
    assert_eq!(r.shape, Shape::SingleFlat);
    assert_eq!(r.body["deleted"], json!(1));
    assert_eq!(cache.info().unwrap().entries, 1);
}
