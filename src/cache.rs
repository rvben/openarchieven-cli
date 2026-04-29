//! On-disk JSON cache for upstream GET responses.
//!
//! Keys are sha256 of `(canonical_base_url, method, path, sorted_query, ttl_class)`.
//! Files are written atomically (`tempfile::persist`) and read without locking.
//! Destructive operations (`clear`, `prune`) acquire an exclusive `fs4` lock.

use crate::error::{Error, ErrorKind, Result};
use chrono::{DateTime, Utc};
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
    debug_assert!(
        !method.contains('\0') && !path.contains('\0') && !ttl_class.contains('\0'),
        "key() inputs must not contain NUL bytes (would collide with field separator)"
    );
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

const LOCK_FILENAME: &str = ".lock";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub url: String,
    pub fetched_at: DateTime<Utc>,
    /// `None` represents `--cache-ttl inf` — never expires.
    pub expires_at: Option<DateTime<Utc>>,
    pub body: serde_json::Value,
}

impl Entry {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(e) => now >= e,
            None => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Ttl {
    Fixed(Duration),
    UntilMidnight,
    Never,
}

impl Ttl {
    pub fn expires_at(self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Ttl::Fixed(d) => Some(now + chrono::Duration::from_std(d).unwrap_or_default()),
            Ttl::UntilMidnight => {
                let next = (now + chrono::Duration::days(1))
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc();
                Some(next)
            }
            Ttl::Never => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
    disabled: bool,
}

impl Cache {
    /// Open (or create) a cache rooted at `root`. `root` must be a real
    /// directory (or non-existent — it will be created with `0700`). Symlinked
    /// roots are rejected.
    pub fn open(root: PathBuf, disabled: bool) -> Result<Self> {
        if !disabled {
            if root.exists() {
                let meta = fs::symlink_metadata(&root).map_err(|e| {
                    Error::new(ErrorKind::Validation, format!("cache-dir stat: {e}"))
                })?;
                if meta.file_type().is_symlink() {
                    return Err(Error::new(
                        ErrorKind::Validation,
                        format!("cache-dir is a symlink: {}", root.display()),
                    ));
                }
                if !meta.is_dir() {
                    return Err(Error::new(
                        ErrorKind::Validation,
                        format!("cache-dir is not a directory: {}", root.display()),
                    ));
                }
            } else {
                fs::create_dir_all(&root).map_err(|e| {
                    Error::new(ErrorKind::Validation, format!("create cache-dir: {e}"))
                })?;
            }
            // Always tighten permissions on the cache root: genealogical query
            // responses are private to the user.
            set_dir_private(&root);
        }
        Ok(Self { root, disabled })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn disabled(&self) -> bool {
        self.disabled
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.json"))
    }

    /// Fetch a non-expired entry for `key`. Corrupted entries are silently
    /// treated as a miss (with an `eprintln!` warning).
    pub fn get(&self, key: &str, now: DateTime<Utc>) -> Option<Entry> {
        if self.disabled {
            return None;
        }
        let path = self.entry_path(key);
        let bytes = fs::read(&path).ok()?;
        match serde_json::from_slice::<Entry>(&bytes) {
            Ok(e) if !e.is_expired(now) => Some(e),
            Ok(_) => None,
            Err(err) => {
                eprintln!("openarchieven: corrupted cache entry {key}: {err}");
                None
            }
        }
    }

    /// Store an entry for `key`. Failures are logged but not propagated —
    /// a write failure must never break a successful read.
    pub fn put(&self, key: &str, entry: &Entry) {
        if self.disabled {
            return;
        }
        if let Err(err) = self.put_inner(key, entry) {
            eprintln!("openarchieven: cache write failed for {key}: {err}");
        }
    }

    fn put_inner(&self, key: &str, entry: &Entry) -> std::io::Result<()> {
        let dest = self.entry_path(key);
        let mut tmp = tempfile::NamedTempFile::new_in(&self.root)?;
        serde_json::to_writer(tmp.as_file_mut(), entry).map_err(std::io::Error::other)?;
        tmp.as_file_mut().flush()?;
        set_file_private(tmp.path());
        tmp.persist(dest).map_err(std::io::Error::other)?;
        Ok(())
    }
}

#[cfg(unix)]
fn set_dir_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(unix)]
fn set_file_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_dir_private(_: &Path) {}
#[cfg(not(unix))]
fn set_file_private(_: &Path) {}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    pub root: PathBuf,
    pub entries: u64,
    pub bytes: u64,
    pub oldest: Option<DateTime<Utc>>,
    pub newest: Option<DateTime<Utc>>,
}

impl Cache {
    /// Snapshot stats. Reads without locking; result may be slightly stale.
    pub fn info(&self) -> Result<Info> {
        let mut entries = 0u64;
        let mut bytes = 0u64;
        let mut oldest: Option<DateTime<Utc>> = None;
        let mut newest: Option<DateTime<Utc>> = None;
        if !self.root.exists() {
            return Ok(Info {
                root: self.root.clone(),
                entries: 0,
                bytes: 0,
                oldest: None,
                newest: None,
            });
        }
        for de in fs::read_dir(&self.root)
            .map_err(|e| Error::new(ErrorKind::Validation, format!("read cache-dir: {e}")))?
        {
            let de =
                de.map_err(|e| Error::new(ErrorKind::Validation, format!("dir entry: {e}")))?;
            if !is_entry_filename(&de.file_name().to_string_lossy()) {
                continue;
            }
            let meta = match de.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if !meta.is_file() {
                continue;
            }
            entries += 1;
            bytes += meta.len();
            if let Ok(bytes_buf) = fs::read(de.path())
                && let Ok(e) = serde_json::from_slice::<Entry>(&bytes_buf)
            {
                oldest = Some(oldest.map_or(e.fetched_at, |o| o.min(e.fetched_at)));
                newest = Some(newest.map_or(e.fetched_at, |n| n.max(e.fetched_at)));
            }
        }
        Ok(Info {
            root: self.root.clone(),
            entries,
            bytes,
            oldest,
            newest,
        })
    }

    /// Delete every cache entry. Acquires an exclusive advisory lock; waits up
    /// to 5s before returning a `validation` error.
    pub fn clear(&self) -> Result<u64> {
        self.with_exclusive_lock(|| {
            let mut deleted = 0u64;
            for de in fs::read_dir(&self.root)
                .map_err(|e| Error::new(ErrorKind::Validation, format!("read cache-dir: {e}")))?
            {
                let de =
                    de.map_err(|e| Error::new(ErrorKind::Validation, format!("dir entry: {e}")))?;
                if !is_entry_filename(&de.file_name().to_string_lossy()) {
                    continue;
                }
                let path = de.path();
                let meta = match fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if !meta.is_file() {
                    continue;
                }
                if fs::remove_file(&path).is_ok() {
                    deleted += 1;
                }
            }
            Ok(deleted)
        })
    }

    /// Delete only expired entries. Corrupted entries are also removed.
    pub fn prune(&self, now: DateTime<Utc>) -> Result<u64> {
        self.with_exclusive_lock(|| {
            let mut deleted = 0u64;
            for de in fs::read_dir(&self.root)
                .map_err(|e| Error::new(ErrorKind::Validation, format!("read cache-dir: {e}")))?
            {
                let de =
                    de.map_err(|e| Error::new(ErrorKind::Validation, format!("dir entry: {e}")))?;
                if !is_entry_filename(&de.file_name().to_string_lossy()) {
                    continue;
                }
                let path = de.path();
                let meta = match fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if !meta.is_file() {
                    continue;
                }
                let bytes = match fs::read(&path) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let entry: Entry = match serde_json::from_slice(&bytes) {
                    Ok(e) => e,
                    Err(_) => {
                        if fs::remove_file(&path).is_ok() {
                            deleted += 1;
                        }
                        continue;
                    }
                };
                if entry.is_expired(now) && fs::remove_file(&path).is_ok() {
                    deleted += 1;
                }
            }
            Ok(deleted)
        })
    }

    fn with_exclusive_lock<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let lock_path = self.root.join(LOCK_FILENAME);
        let already_existed = lock_path.exists();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| Error::new(ErrorKind::Validation, format!("lock open: {e}")))?;
        if !already_existed {
            set_file_private(&lock_path);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if Instant::now() >= deadline {
                return Err(Error::new(
                    ErrorKind::Validation,
                    "another openarchieven cache operation is in progress",
                ));
            }
            match lock.try_lock_exclusive() {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    return Err(Error::new(
                        ErrorKind::Validation,
                        format!("lock acquire: {e}"),
                    ));
                }
            }
        }
        let result = f();
        // The kernel releases the flock when `lock` is dropped (including on
        // panic unwind), so this explicit unlock is just early cleanup.
        let _ = lock.unlock();
        result
    }
}

/// Returns true for `<64-hex>.json` filenames produced by [`Cache::key`].
/// Used by `info`, `clear`, and `prune` to skip foreign files (`.lock`,
/// `README.txt`, partial `tempfile`s, etc.) when scanning the cache root.
fn is_entry_filename(name: &str) -> bool {
    name.len() == 69 && name.ends_with(".json") && name[..64].chars().all(|c| c.is_ascii_hexdigit())
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
    fn key_changes_with_method() {
        let a = key("https://example.com", "GET", "/x", &p(&[]), "");
        let b = key("https://example.com", "POST", "/x", &p(&[]), "");
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
    fn key_normalizes_default_port() {
        let a = key("https://example.com:443/1.1", "GET", "/x", &p(&[]), "");
        let b = key("https://example.com/1.1", "GET", "/x", &p(&[]), "");
        assert_eq!(a, b);
    }

    #[test]
    fn key_invalid_url_falls_back_to_trailing_slash_strip() {
        let a = key("not-a-url/", "GET", "/x", &p(&[]), "");
        let b = key("not-a-url", "GET", "/x", &p(&[]), "");
        assert_eq!(a, b);
    }

    #[test]
    fn key_is_64_hex_chars() {
        let k = key("https://e.com", "GET", "/x", &p(&[]), "");
        assert_eq!(k.len(), 64);
        assert!(k.chars().all(|c| c.is_ascii_hexdigit()));
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;
    use serde_json::json;

    fn td() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn entry(now: DateTime<Utc>, ttl: Ttl) -> Entry {
        Entry {
            url: "https://example.com/x".into(),
            fetched_at: now,
            expires_at: ttl.expires_at(now),
            body: json!({"a": 1}),
        }
    }

    #[test]
    fn put_then_get_returns_same_body() {
        let dir = td();
        let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
        let now = Utc::now();
        let e = entry(now, Ttl::Fixed(Duration::from_secs(60)));
        cache.put("abc", &e);
        let back = cache.get("abc", now).unwrap();
        assert_eq!(back.body, json!({"a": 1}));
    }

    #[test]
    fn expired_entry_returns_none() {
        let dir = td();
        let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
        let now = Utc::now();
        let mut e = entry(now, Ttl::Fixed(Duration::from_secs(60)));
        e.expires_at = Some(now - chrono::Duration::seconds(1));
        cache.put("abc", &e);
        assert!(cache.get("abc", now).is_none());
    }

    #[test]
    fn corrupted_entry_treated_as_miss() {
        let dir = td();
        let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
        std::fs::write(dir.path().join("abc.json"), "not json").unwrap();
        assert!(cache.get("abc", Utc::now()).is_none());
    }

    #[test]
    fn never_expires_returns_entry_far_future() {
        let dir = td();
        let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
        let now = Utc::now();
        let e = entry(now, Ttl::Never);
        cache.put("abc", &e);
        let far = now + chrono::Duration::days(365 * 100);
        assert!(cache.get("abc", far).is_some());
    }

    #[test]
    fn until_midnight_expires_at_next_calendar_midnight() {
        let now = Utc::now();
        let exp = Ttl::UntilMidnight.expires_at(now).unwrap();
        assert_eq!(
            exp.time(),
            chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()
        );
        assert_eq!(
            exp.date_naive(),
            (now + chrono::Duration::days(1)).date_naive()
        );
    }

    #[test]
    fn is_expired_at_exact_boundary() {
        let now = Utc::now();
        let e = Entry {
            url: "u".into(),
            fetched_at: now,
            expires_at: Some(now),
            body: serde_json::json!({}),
        };
        assert!(e.is_expired(now));
    }

    #[test]
    fn put_overwrites_existing_key() {
        let dir = td();
        let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
        let now = Utc::now();
        let mut e = entry(now, Ttl::Fixed(Duration::from_secs(60)));
        cache.put("abc", &e);
        e.body = json!({"a": 2});
        cache.put("abc", &e);
        assert_eq!(cache.get("abc", now).unwrap().body, json!({"a": 2}));
    }

    #[test]
    fn rejects_regular_file_as_root() {
        let dir = td();
        let path = dir.path().join("not-a-dir");
        std::fs::write(&path, b"hello").unwrap();
        let err = Cache::open(path, false).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
    }

    #[test]
    fn disabled_cache_is_a_noop() {
        let dir = td();
        let cache = Cache::open(dir.path().to_path_buf(), true).unwrap();
        let now = Utc::now();
        cache.put("abc", &entry(now, Ttl::Fixed(Duration::from_secs(60))));
        assert!(cache.get("abc", now).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_root() {
        let dir = td();
        let target = dir.path().join("real");
        std::fs::create_dir(&target).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let err = Cache::open(link, false).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
    }
}

#[cfg(test)]
mod ops_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn info_counts_entries_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
        let older = Utc::now() - chrono::Duration::hours(2);
        let newer = Utc::now();
        let entry_old = Entry {
            url: "u".into(),
            fetched_at: older,
            expires_at: Some(older + chrono::Duration::hours(1)),
            body: json!({}),
        };
        let entry_new = Entry {
            url: "u".into(),
            fetched_at: newer,
            expires_at: Some(newer + chrono::Duration::hours(1)),
            body: json!({}),
        };
        cache.put("a".repeat(64).as_str(), &entry_old);
        cache.put("b".repeat(64).as_str(), &entry_new);
        let info = cache.info().unwrap();
        assert_eq!(info.entries, 2);
        assert!(info.bytes > 0);
        assert_eq!(info.oldest, Some(older));
        assert_eq!(info.newest, Some(newer));
    }

    #[test]
    fn clear_removes_only_entry_files() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
        let entry = Entry {
            url: "u".into(),
            fetched_at: Utc::now(),
            expires_at: None,
            body: json!({}),
        };
        cache.put("a".repeat(64).as_str(), &entry);
        std::fs::write(dir.path().join("README.txt"), "keep me").unwrap();
        let n = cache.clear().unwrap();
        assert_eq!(n, 1);
        assert!(dir.path().join("README.txt").exists());
        // .lock is created by clear() itself and must persist (so a concurrent
        // operation can still observe it).
        assert!(dir.path().join(".lock").exists());
    }

    #[test]
    fn prune_keeps_fresh_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
        let now = Utc::now();
        let fresh = Entry {
            url: "u".into(),
            fetched_at: now,
            expires_at: Some(now + chrono::Duration::hours(1)),
            body: json!({}),
        };
        let stale = Entry {
            url: "u".into(),
            fetched_at: now - chrono::Duration::days(1),
            expires_at: Some(now - chrono::Duration::hours(1)),
            body: json!({}),
        };
        cache.put("a".repeat(64).as_str(), &fresh);
        cache.put("b".repeat(64).as_str(), &stale);
        let n = cache.prune(now).unwrap();
        assert_eq!(n, 1);
        assert_eq!(cache.info().unwrap().entries, 1);
    }

    #[test]
    fn prune_removes_corrupted_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path().to_path_buf(), false).unwrap();
        std::fs::write(
            dir.path().join(format!("{}.json", "c".repeat(64))),
            "not json",
        )
        .unwrap();
        let n = cache.prune(Utc::now()).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn entry_filename_predicate() {
        assert!(is_entry_filename(&format!("{}.json", "a".repeat(64))));
        assert!(!is_entry_filename(".lock"));
        assert!(!is_entry_filename("hello.json"));
        assert!(!is_entry_filename(&format!("{}.json", "Z".repeat(64))));
    }
}
