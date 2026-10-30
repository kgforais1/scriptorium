//! Reading-progress bookkeeping for the books opened in this run.
//!
//! Pure logic with no Tauri types, so the debounce and the close flush are unit-testable.
//! Books are keyed by canonical package root, never by the self-declared id, so two
//! packages with the same id never cross. Callers canonicalize with [`canonical_key`].

use crate::library::command_error::CommandError;
use crate::library::{io_error, BookError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Least time between two `meta.json` writes for one book while pages are being turned.
pub const SAVE_INTERVAL: Duration = Duration::from_secs(1);

/// Canonicalize `path` so `dir/`, `dir/../dir` and a symlinked path share one key.
pub fn canonical_key(path: &Path) -> Result<PathBuf, BookError> {
    path.canonicalize().map_err(|e| io_error(path, &e))
}

/// What the session remembers about one open book.
#[derive(Debug)]
struct Entry {
    /// The page the reader is on.
    page: u32,
    /// The book's page count, so an out-of-range page is refused before it is stored.
    page_count: u32,
    /// True while `page` has not been written to `meta.json`.
    dirty: bool,
    /// When this book's `meta.json` was last written (or the book was opened).
    last_write: Instant,
}

/// Why [`Session::record`] refused a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    /// The root was never opened in this session.
    NotOpen,
    /// The page is past the book's last page.
    PageOutOfRange {
        /// The page that was asked for.
        page: u32,
        /// How many pages the book has.
        page_count: u32,
    },
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOpen => write!(f, "that book was not opened in this session"),
            Self::PageOutOfRange { page, page_count } => {
                write!(f, "page {page} is past the last page ({})", page_count - 1)
            }
        }
    }
}

impl From<RecordError> for CommandError {
    /// A page past the end is a real bound on the book. A book that was never opened is a
    /// slip by our own webview, so it is `Internal`, not a fault in the book.
    fn from(err: RecordError) -> Self {
        match err {
            RecordError::NotOpen => CommandError::internal(err.to_string()),
            RecordError::PageOutOfRange { .. } => BookError::InvalidPackage {
                reason: err.to_string(),
            }
            .into(),
        }
    }
}

/// The open books of this run and their unsaved reading positions.
#[derive(Debug)]
pub struct Session {
    interval: Duration,
    books: HashMap<PathBuf, Entry>,
}

impl Session {
    /// An empty session that writes each book at most once per `interval`.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            books: HashMap::new(),
        }
    }

    /// Start tracking the book at canonical `root`, at `page`, with nothing unsaved.
    /// A book that is already tracked is reset to the state just read from disk.
    pub fn register(&mut self, root: PathBuf, page: u32, page_count: u32, now: Instant) {
        self.books.insert(
            root,
            Entry {
                page,
                page_count,
                dirty: false,
                last_write: now,
            },
        );
    }

    /// Remember that the reader is on `page` and mark the book dirty. Returns true when at
    /// least one interval has passed since the last write, so the caller should flush now.
    pub fn record(&mut self, root: &Path, page: u32, now: Instant) -> Result<bool, RecordError> {
        let entry = self.books.get_mut(root).ok_or(RecordError::NotOpen)?;
        if page >= entry.page_count {
            return Err(RecordError::PageOutOfRange {
                page,
                page_count: entry.page_count,
            });
        }
        entry.page = page;
        entry.dirty = true;
        Ok(now.saturating_duration_since(entry.last_write) >= self.interval)
    }

    /// Write every dirty book (or only `only`, if given) with `write(root, page)`. A book
    /// whose write succeeds becomes clean and restarts its debounce at `now`. A book whose
    /// write fails stays dirty. Returns the failures; nothing here panics.
    pub fn flush<E>(
        &mut self,
        only: Option<&Path>,
        now: Instant,
        mut write: impl FnMut(&Path, u32) -> Result<(), E>,
    ) -> Vec<(PathBuf, E)> {
        let mut failures = Vec::new();
        for (root, entry) in &mut self.books {
            if !entry.dirty || only.is_some_and(|p| p != root.as_path()) {
                continue;
            }
            match write(root, entry.page) {
                Ok(()) => {
                    entry.dirty = false;
                    entry.last_write = now;
                }
                Err(e) => failures.push((root.clone(), e)),
            }
        }
        failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: Duration = Duration::from_secs(1);

    fn session_with(roots: &[&str], t0: Instant) -> Session {
        let mut s = Session::new(SEC);
        for r in roots {
            s.register(PathBuf::from(r), 0, 10, t0);
        }
        s
    }

    fn ok_writer(log: &mut Vec<(PathBuf, u32)>) -> impl FnMut(&Path, u32) -> Result<(), ()> + '_ {
        |root, page| {
            log.push((root.to_path_buf(), page));
            Ok(())
        }
    }

    #[test]
    fn write_is_due_only_after_the_interval() {
        let t0 = Instant::now();
        let mut s = session_with(&["/a"], t0);
        let a = Path::new("/a");
        assert_eq!(s.record(a, 1, t0 + SEC / 2), Ok(false));
        assert_eq!(s.record(a, 2, t0 + SEC), Ok(true));
        let mut log = Vec::new();
        assert!(s.flush(Some(a), t0 + SEC, ok_writer(&mut log)).is_empty());
        assert_eq!(log, [(PathBuf::from("/a"), 2)], "writes the latest page");
        // The debounce restarts from the write.
        assert_eq!(s.record(a, 3, t0 + SEC + SEC / 2), Ok(false));
        assert_eq!(s.record(a, 4, t0 + SEC * 2), Ok(true));
    }

    #[test]
    fn flush_writes_every_dirty_book_once_and_skips_clean_ones() {
        let t0 = Instant::now();
        let mut s = session_with(&["/a", "/b", "/c"], t0);
        s.record(Path::new("/a"), 3, t0).unwrap();
        s.record(Path::new("/c"), 7, t0).unwrap();
        let mut log = Vec::new();
        assert!(s.flush(None, t0, ok_writer(&mut log)).is_empty());
        log.sort();
        assert_eq!(log, [(PathBuf::from("/a"), 3), (PathBuf::from("/c"), 7)]);
        let mut again = Vec::new();
        s.flush(None, t0, ok_writer(&mut again));
        assert!(again.is_empty(), "a second flush has nothing to write");
    }

    #[test]
    fn a_failed_write_stays_dirty_and_is_reported() {
        let t0 = Instant::now();
        let mut s = session_with(&["/a"], t0);
        s.record(Path::new("/a"), 2, t0).unwrap();
        let failures = s.flush(None, t0, |_, _| Err("disk full"));
        assert_eq!(failures, [(PathBuf::from("/a"), "disk full")]);
        let mut log = Vec::new();
        s.flush(None, t0, ok_writer(&mut log));
        assert_eq!(log, [(PathBuf::from("/a"), 2)], "still dirty, so retried");
    }

    #[test]
    fn unknown_roots_and_bad_pages_are_refused() {
        let t0 = Instant::now();
        let mut s = session_with(&["/a"], t0);
        assert_eq!(
            s.record(Path::new("/zzz"), 0, t0),
            Err(RecordError::NotOpen)
        );
        assert_eq!(
            s.record(Path::new("/a"), 10, t0),
            Err(RecordError::PageOutOfRange {
                page: 10,
                page_count: 10
            })
        );
        // A refused page leaves the book clean.
        let mut log = Vec::new();
        s.flush(None, t0, ok_writer(&mut log));
        assert!(log.is_empty());
    }

    #[test]
    fn spellings_of_one_directory_share_one_key() {
        let dir = std::env::temp_dir().join(format!("scriptorium-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = dir.file_name().unwrap().to_owned();
        let plain = canonical_key(&dir).unwrap();
        let slash = canonical_key(&PathBuf::from(format!("{}/", dir.display()))).unwrap();
        let dotdot = canonical_key(&dir.join("..").join(&name)).unwrap();
        assert_eq!(plain, slash);
        assert_eq!(plain, dotdot);

        let t0 = Instant::now();
        let mut s = Session::new(SEC);
        s.register(plain, 0, 5, t0);
        assert!(s.record(&slash, 1, t0).is_ok());
        assert!(s.record(&dotdot, 2, t0).is_ok());
        let mut log = Vec::new();
        s.flush(None, t0, ok_writer(&mut log));
        assert_eq!(log.len(), 1, "one entry, not three");
        assert_eq!(log[0].1, 2);
        assert!(canonical_key(&dir.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
