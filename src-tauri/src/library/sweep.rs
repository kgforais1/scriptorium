//! Removing temp directories left by an ingest that crashed.

use std::path::Path;
use std::time::Duration;

/// Prefix of an in-progress ingest directory under the library root.
pub const TMP_PREFIX: &str = ".ingest-tmp-";
/// Suffix of other Scriptorium temp entries (atomic `meta.json` writes).
pub const TMP_SUFFIX: &str = ".scriptorium-tmp";
/// Only entries older than this are removed, so a concurrent ingest is never disturbed.
pub const ORPHAN_AGE: Duration = Duration::from_secs(60 * 60);

/// Delete orphaned temp entries directly under `library` older than `max_age`. Returns how
/// many were removed. Errors are ignored: a sweep must never block ingest or startup.
pub fn sweep_orphans(library: &Path, max_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(library) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with(TMP_PREFIX) || name.ends_with(TMP_SUFFIX)) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age >= max_age);
        if !old {
            continue;
        }
        let path = entry.path();
        let ok = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if ok.is_ok() {
            removed += 1;
        }
    }
    removed
}
