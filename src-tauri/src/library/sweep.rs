//! Removing temp directories left by an ingest that crashed.

use std::path::Path;
use std::time::Duration;

/// Prefix of an in-progress ingest directory under the library root.
pub const TMP_PREFIX: &str = ".ingest-tmp-";
/// Suffix of other Scriptorium temp entries (atomic `meta.json` writes).
pub const TMP_SUFFIX: &str = ".scriptorium-tmp";
/// Only entries older than this are removed, so a concurrent ingest is never disturbed.
pub const ORPHAN_AGE: Duration = Duration::from_secs(60 * 60);

/// Name of the lock file an ingest holds inside its temp directory while it runs.
pub const INGEST_LOCK: &str = ".ingest.lock";

/// Create and exclusively lock the ingest lock file in `dir`. Keep the returned file open for
/// the whole build; the OS drops the lock if the process crashes.
pub(crate) fn lock_ingest_dir(dir: &Path) -> Result<std::fs::File, super::BookError> {
    let path = dir.join(INGEST_LOCK);
    let file = std::fs::File::create(&path).map_err(|e| super::io_error(&path, &e))?;
    file.lock().map_err(|e| super::io_error(&path, &e))?;
    Ok(file)
}

/// True while another ingest holds `dir`'s lock. A missing lock file means the ingest died
/// before taking it, so the directory is litter.
fn ingest_running(dir: &Path) -> bool {
    match std::fs::File::open(dir.join(INGEST_LOCK)) {
        Ok(file) => file.try_lock().is_err(),
        Err(_) => false,
    }
}

/// Delete orphaned temp entries under `library` older than `max_age`. Directories are removed
/// only when they are ingest temp dirs (`.ingest-tmp-*`), which a book id can never be, and only
/// when no running ingest holds their lock. Stray temp files are removed at the top level and
/// inside each book. A directory that merely ends in the temp suffix is treated as a book,
/// whether or not its `meta.json` survives. Returns how many entries were removed. Errors are
/// ignored: a sweep must never block ingest or startup.
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
        let path = entry.path();
        if kind.is_dir() && name.starts_with(TMP_PREFIX) {
            if is_old(&entry, max_age) && !ingest_running(&path) {
                removed += usize::from(std::fs::remove_dir_all(&path).is_ok());
            }
        } else if kind.is_file() && name.ends_with(TMP_SUFFIX) {
            if is_old(&entry, max_age) {
                removed += usize::from(std::fs::remove_file(&path).is_ok());
            }
        } else if kind.is_dir() {
            removed += sweep_book(&path, max_age);
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
