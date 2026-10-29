//! Removing temp directories left by an ingest that crashed.

use std::path::Path;
use std::time::Duration;

/// Prefix of an in-progress ingest directory under the library root.
pub const TMP_PREFIX: &str = ".ingest-tmp-";
/// Suffix of other Scriptorium temp entries (atomic `meta.json` writes).
pub const TMP_SUFFIX: &str = ".scriptorium-tmp";
/// Only entries older than this are removed, so a concurrent ingest is never disturbed.
pub const ORPHAN_AGE: Duration = Duration::from_secs(60 * 60);

/// Delete orphaned temp entries under `library` older than `max_age`: ingest directories and
/// stray temp files at the top level, and temp files a crashed save left inside a book.
/// Returns how many were removed. Errors are ignored: a sweep must never block ingest or
/// startup.
pub fn sweep_orphans(library: &Path, max_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(library) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // A finished book whose id happens to end in the suffix is a book, not litter. Book ids
        // start with [a-z0-9], so an ingest temp dir (".ingest-tmp-…") is never mistaken for
        // one, even after a crash left a meta.json inside it.
        let is_book = kind.is_dir()
            && !name.starts_with(TMP_PREFIX)
            && entry.path().join("meta.json").is_file();
        if !is_book && (name.starts_with(TMP_PREFIX) || name.ends_with(TMP_SUFFIX)) {
            if !is_old(&entry, max_age) {
                continue;
            }
            let path = entry.path();
            let ok = if kind.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            removed += usize::from(ok.is_ok());
        } else if kind.is_dir() {
            removed += sweep_book(&entry.path(), max_age);
        }
    }
    removed
}

fn is_old(entry: &std::fs::DirEntry, max_age: Duration) -> bool {
    entry
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age >= max_age)
}

/// Remove old `*.scriptorium-tmp` files (never directories) directly inside one book.
fn sweep_book(book: &Path, max_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(book) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| e.file_name().to_string_lossy().ends_with(TMP_SUFFIX) && is_old(e, max_age))
        .filter(|e| std::fs::remove_file(e.path()).is_ok())
        .count()
}
