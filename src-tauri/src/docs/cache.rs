//! On-disk docs cache: `<cache_dir>/<lang>/<path>` plus a sidecar `<path>.stamp` holding the
//! unix time (seconds) at which the file was stored. The sidecar is used instead of the file
//! mtime because mtimes are unreliable across copies, backups and some file systems.
//!
//! All writes are best-effort: a failing cache never fails a docs request.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A cached file together with the time it was stored (unix seconds; `0` when unknown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedFile {
    pub text: String,
    pub stored_at: u64,
}

/// `true` while a file stored at `stored_at` is still within `ttl_secs` of `now`.
/// A TTL of `0` disables the cache-first path (the cache is only used as stale fallback);
/// an unknown stamp (`stored_at == 0`) is always stale.
pub fn is_fresh(stored_at: u64, now: u64, ttl_secs: u64) -> bool {
    stored_at > 0 && ttl_secs > 0 && now.saturating_sub(stored_at) < ttl_secs
}

/// Current unix time in seconds (`0` if the clock is before 1970).
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Location of `rel_path` for `lang` inside `dir`. Callers validate `rel_path` first.
pub fn file_path(dir: &Path, lang: &str, rel_path: &str) -> PathBuf {
    let mut p = dir.join(lang);
    for segment in rel_path.split('/') {
        p.push(segment);
    }
    p
}

fn stamp_path(file: &Path) -> PathBuf {
    let mut name = file
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".stamp");
    file.with_file_name(name)
}

/// Reads the cached file (and its stamp) if present. A missing or unparsable stamp yields
/// `stored_at == 0`, i.e. the entry is treated as stale.
pub async fn read(dir: &Path, lang: &str, rel_path: &str) -> Option<CachedFile> {
    let file = file_path(dir, lang, rel_path);
    let text = tokio::fs::read_to_string(&file).await.ok()?;
    let stored_at = tokio::fs::read_to_string(stamp_path(&file))
        .await
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    Some(CachedFile { text, stored_at })
}

/// Writes `text` and a fresh stamp. Creates parent directories as needed.
pub async fn write(dir: &Path, lang: &str, rel_path: &str, text: &str) -> std::io::Result<()> {
    let file = file_path(dir, lang, rel_path);
    if let Some(parent) = file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&file, text).await?;
    tokio::fs::write(stamp_path(&file), now_secs().to_string()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_respects_ttl_and_clock() {
        // (stored_at, now, ttl) -> fresh?
        let cases = [
            (1_000, 1_000, 3_600, true),  // just stored
            (1_000, 4_599, 3_600, true),  // one second before expiry
            (1_000, 4_600, 3_600, false), // exactly at expiry
            (1_000, 9_999, 3_600, false), // long expired
            (5_000, 1_000, 3_600, true),  // clock went backwards: treat as fresh
            (0, 1_000, 3_600, false),     // unknown stamp -> stale
            (1_000, 1_000, 0, false),     // ttl 0 disables cache-first
        ];
        for (stored, now, ttl, expected) in cases {
            assert_eq!(is_fresh(stored, now, ttl), expected, "{stored} {now} {ttl}");
        }
    }

    #[test]
    fn stamp_lives_next_to_the_file() {
        let file = Path::new("cache")
            .join("en")
            .join("guides")
            .join("verify.md");
        let stamp = stamp_path(&file);
        assert_eq!(stamp.parent(), file.parent());
        assert_eq!(
            stamp.file_name().and_then(|n| n.to_str()),
            Some("verify.md.stamp")
        );
    }

    #[test]
    fn file_path_nests_lang_and_segments() {
        let p = file_path(Path::new("root"), "zh-CN", "a/b.md");
        assert_eq!(p, Path::new("root").join("zh-CN").join("a").join("b.md"));
    }

    #[tokio::test]
    async fn write_then_read_round_trips_with_fresh_stamp() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "en", "sub/page.md", "# Hi")
            .await
            .expect("write");
        let cached = read(dir.path(), "en", "sub/page.md").await.expect("cached");
        assert_eq!(cached.text, "# Hi");
        assert!(is_fresh(cached.stored_at, now_secs(), 60));
        assert!(read(dir.path(), "en", "nope.md").await.is_none());
    }
}
