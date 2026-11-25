//! The OCR job's per-run temp directory, and removal of ones left by a crash.
//!
//! A run directory holds the page inputs, TSV, and stderr files for the pages in flight. It is
//! removed when its guard drops. A crashed run leaves its directory and lock behind, so
//! [`sweep_stale`] removes old ones whose lock is free.

use crate::library::sweep::RUN_LOCK;
use crate::library::{io_error, BookError};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Prefix of a run directory under the system temp directory.
pub(crate) const RUN_PREFIX: &str = "scriptorium-ocr-run-";
/// Only run directories older than this are swept, so a live run is never disturbed.
pub(crate) const RUN_ORPHAN_AGE: Duration = Duration::from_secs(60 * 60);
/// How many names to try when a run directory name is taken.
const NAME_ATTEMPTS: u128 = 16;

/// A temp directory for one OCR run, removed when this guard drops.
pub(crate) struct RunDir {
    path: PathBuf,
    lock: Option<File>,
}

impl RunDir {
    /// Create a fresh run directory under the system temp directory.
    pub(crate) fn new() -> Result<Self, BookError> {
        Self::create_in(&std::env::temp_dir(), now_nanos())
    }

    /// The directory page files are written to.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Create a run directory under `base`, retrying with a new name when one is taken.
    fn create_in(base: &Path, nanos: u128) -> Result<Self, BookError> {
        for attempt in 0..NAME_ATTEMPTS {
            let path = base.join(format!(
                "{RUN_PREFIX}{}-{}",
                std::process::id(),
                nanos + attempt
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    // Nothing can exist inside a fresh directory, so create_new is safe here.
                    let lock = match create_lock(&path) {
                        Ok(lock) => lock,
                        Err(error) => {
                            let _ = std::fs::remove_dir_all(&path);
                            return Err(error);
                        }
                    };
                    return Ok(Self {
                        path,
                        lock: Some(lock),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error(&path, &e)),
            }
        }
        Err(BookError::OcrStorage {
            path: base.to_path_buf(),
            error: "no unique temporary directory name".into(),
        })
    }
}

impl Drop for RunDir {
    fn drop(&mut self) {
        drop(self.lock.take());
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Create and lock the run directory's lock file. The directory is fresh, so `create_new`
/// proves nothing created the file first.
fn create_lock(dir: &Path) -> Result<File, BookError> {
    let path = dir.join(RUN_LOCK);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| io_error(&path, &e))?;
    file.lock().map_err(|e| io_error(&path, &e))?;
    Ok(file)
}

/// Remove run directories under `base` older than `max_age` whose lock is free.
///
/// Only the top level is read, only names with [`RUN_PREFIX`] are considered, symlinks are never
/// followed, and a directory whose lock another process holds is left alone. Errors are ignored:
/// a sweep must never block startup or a run.
pub(crate) fn sweep_stale(base: &Path, max_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(base) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(RUN_PREFIX) {
            continue;
        }
        // `file_type` does not follow a symlink, so a link is neither a directory nor removed.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() || !is_old(&entry, max_age) {
            continue;
        }
        let path = entry.path();
        if !lock_is_free(&path) {
            continue;
        }
        removed += usize::from(std::fs::remove_dir_all(&path).is_ok());
    }
    removed
}

/// The clock's nanoseconds since the epoch, matching `library/create.rs`.
fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Whether `entry`'s modification time is at least `max_age` old.
fn is_old(entry: &std::fs::DirEntry, max_age: Duration) -> bool {
    entry
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age >= max_age)
}

/// True when no process holds the run directory's lock. A missing lock file means the run died
/// before creating it, so an old directory is safe to remove.
fn lock_is_free(dir: &Path) -> bool {
    match File::open(dir.join(RUN_LOCK)) {
        Ok(file) => file.try_lock().is_ok(),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test base directory, removed on drop.
    struct Base(PathBuf);

    impl Base {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "scriptorium-ocr-run-test-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn run_dir(&self, label: &str) -> PathBuf {
            let path = self.0.join(format!("{RUN_PREFIX}{label}"));
            std::fs::create_dir(&path).unwrap();
            path
        }
    }

    impl Drop for Base {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_run_dir_holds_its_lock_and_is_removed_on_drop() {
        let base = Base::new("guard");
        let dir = RunDir::create_in(&base.0, 1).unwrap();
        let path = dir.path().to_path_buf();
        assert!(path.is_dir());
        assert!(path.join(RUN_LOCK).is_file());
        // Its lock is held, so even an "old" directory is left alone.
        assert_eq!(sweep_stale(&base.0, Duration::ZERO), 0);
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn a_name_clash_retries_with_a_new_directory() {
        let base = Base::new("clash");
        let taken = base.run_dir(&format!("{}-7", std::process::id()));
        let dir = RunDir::create_in(&base.0, 7).unwrap();
        assert_ne!(dir.path(), taken);
        assert!(taken.is_dir(), "the existing directory is untouched");
    }

    #[test]
    fn sweep_removes_only_old_unlocked_run_dirs() {
        let base = Base::new("sweep");
        let stale = base.run_dir("stale");
        let live = base.run_dir("live");
        let held = File::create(live.join(RUN_LOCK)).unwrap();
        held.lock().unwrap();
        let other = base.0.join("not-a-run-dir");
        std::fs::create_dir(&other).unwrap();

        // A zero age makes every entry old; the live lock is what protects it.
        assert_eq!(sweep_stale(&base.0, Duration::ZERO), 1);
        assert!(!stale.exists());
        assert!(live.exists());
        assert!(other.exists());
    }

    #[cfg(unix)]
    #[test]
    fn sweep_does_not_follow_a_symlink_with_the_prefix() {
        let base = Base::new("link");
        let target = base.0.join("target");
        std::fs::create_dir(&target).unwrap();
        let link = base.0.join(format!("{RUN_PREFIX}link"));
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert_eq!(sweep_stale(&base.0, Duration::ZERO), 0);
        assert!(target.is_dir());
    }
}
