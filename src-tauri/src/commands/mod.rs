//! Tauri commands: the only way the webview touches books.
//!
//! This file is thin glue. The logic that can be tested without a Tauri runtime lives in
//! [`session`] (debounce and close flush), [`state_file`] (`state.json`), [`throttle`]
//! (progress events) and [`view`] (what the webview receives). Each command runs its file
//! work in `spawn_blocking`, and the session lock is never held across an `.await`.

mod session;
mod state_file;
mod throttle;
mod view;

#[cfg(test)]
mod tests;

use crate::library::command_error::CommandError;
use crate::library::create::{create_book_from_folder_with, CreateOptions};
use crate::library::inspect::{self, FolderInfo};
use crate::library::load::load_book;
use crate::library::progress::save_last_read_page;
use crate::library::sweep::{sweep_orphans, ORPHAN_AGE};
use serde::Serialize;
use session::{canonical_key, Session, SAVE_INTERVAL};
use state_file::{read_last_book, write_last_book};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};
use throttle::{EmitThrottle, EMIT_INTERVAL};
use view::{book_view, BookView};

/// Where the app keeps its files, resolved once in [`setup`].
struct Paths {
    /// `app_data_dir()/books`: one folder per book.
    library: PathBuf,
    /// `app_data_dir()/state.json`.
    state_file: PathBuf,
}

/// Payload of the `ingest-progress` event.
#[derive(Debug, Clone, Serialize)]
struct IngestProgress {
    done: u32,
    total: u32,
}

/// Resolve the library folder, create it, register the app state, and sweep stale ingest
/// temp files once on a background thread.
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let library = data_dir.join("books");
    std::fs::create_dir_all(&library)?;
    let swept = library.clone();
    app.manage(Paths {
        library,
        state_file: data_dir.join("state.json"),
    });
    app.manage(Mutex::new(Session::new(SAVE_INTERVAL)));
    // Never a zero age: another instance may be between making its temp dir and locking it.
    std::thread::spawn(move || {
        sweep_orphans(&swept, ORPHAN_AGE);
    });
    Ok(())
}

/// Write every unsaved reading position to its `meta.json`, now. Called when the window
/// closes and the app exits. Errors are logged and ignored, and nothing here panics.
pub fn flush_all(app: &AppHandle) {
    flush_session(app, None);
}

fn lock(session: &Mutex<Session>) -> MutexGuard<'_, Session> {
    // A panic elsewhere must not stop progress from being saved.
    session.lock().unwrap_or_else(|e| e.into_inner())
}

/// Flush dirty books (all, or only `only`) through `save_last_read_page`. Returns the first
/// failure, if any; every failure is logged.
///
/// The session lock is held across the write on purpose. It is what keeps two writers from
/// interleaving their read-modify-write of one `meta.json`. Narrow it only after adding a
/// per-book write guard.
fn flush_session(app: &AppHandle, only: Option<&Path>) -> Option<CommandError> {
    let session = app.try_state::<Mutex<Session>>()?;
    let failures = lock(&session).flush(only, Instant::now(), |root, page| {
        save_last_read_page(root, page).map(|_| ())
    });
    for (root, err) in &failures {
        eprintln!(
            "could not save reading position for {}: {err}",
            root.display()
        );
    }
    failures.into_iter().next().map(|(_, e)| e.into())
}

/// Run `work` on the blocking pool. A task that panicked or was cancelled becomes an
/// `Internal` error.
async fn blocking<T, F>(work: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CommandError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| CommandError::internal(format!("a background task failed: {e}")))?
}

/// The one way a book becomes open: canonicalize, load and verify, admit its page files to the
/// asset protocol (never `originals/`), track it in the session, and remember it as the last
/// book. A book already open is flushed first, so its saved page is not stale. If that flush
/// fails, the session keeps the unsaved page and the view opens on it.
fn open_and_register(app: &AppHandle, root: &Path) -> Result<BookView, CommandError> {
    let root = canonical_key(root)?;
    flush_session(app, Some(&root));
    let book = load_book(&root)?;
    let mut view = book_view(&book)?;
    // Admit exactly the page files `load_book` verified, in the same simplified form the view
    // hands to the webview, so URL and scope agree. An undeclared file in `pages/`, and
    // everything else in the package (such as `originals/`), stays out of reach.
    let scope = app.asset_protocol_scope();
    for page in &view.pages {
        scope
            .allow_file(&page.path)
            .map_err(|e| CommandError::internal(format!("could not admit a page: {e}")))?;
    }
    let page_count = u32::try_from(book.meta.pages.len()).unwrap_or(u32::MAX);
    view.last_read_page = lock(&app.state::<Mutex<Session>>()).register(
        book.root.clone(),
        book.meta.last_read_page,
        page_count,
        Instant::now(),
    );
    // Failing to remember the book must not fail the open.
    if let Err(e) = write_last_book(&app.state::<Paths>().state_file, &book.root) {
        eprintln!("could not write state.json: {e}");
    }
    Ok(view)
}

/// Say what is in a folder: a Scriptorium book, or scans that could become one.
#[tauri::command]
pub async fn inspect_folder(app: AppHandle, path: String) -> Result<FolderInfo, CommandError> {
    blocking(move || {
        let library = &app.state::<Paths>().library;
        Ok(inspect::inspect_folder(Path::new(&path), library)?)
    })
    .await
}

/// Build a book from a folder of scans and open it. Emits `ingest-progress`
/// `{ done, total }` at most about ten times a second, and always once at `done == total`.
#[tauri::command]
pub async fn create_book(
    app: AppHandle,
    folder: String,
    title: String,
    id: String,
    rights: String,
    attribution: String,
) -> Result<BookView, CommandError> {
    blocking(move || {
        let opts = CreateOptions {
            id,
            title,
            rights,
            attribution,
        };
        let mut throttle = EmitThrottle::new(EMIT_INTERVAL);
        let emitter = app.clone();
        let library = app.state::<Paths>().library.clone();
        let dest =
            create_book_from_folder_with(Path::new(&folder), &library, &opts, |done, total| {
                if throttle.should_emit(done, total, Instant::now()) {
                    // A webview that went away is not an ingest failure.
                    let _ = emitter.emit("ingest-progress", IngestProgress { done, total });
                }
            })?;
        // A book that was built but cannot be opened would block a retry with the same id,
        // so remove it before reporting the error.
        open_and_register(&app, &dest).inspect_err(|_| {
            if let Err(e) = std::fs::remove_dir_all(&dest) {
                eprintln!("could not remove {}: {e}", dest.display());
            }
        })
    })
    .await
}

/// Open the book package at `path` in place. It is never copied.
#[tauri::command]
pub async fn open_book(app: AppHandle, path: String) -> Result<BookView, CommandError> {
    blocking(move || open_and_register(&app, Path::new(&path))).await
}

/// What `save_progress` did with the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveOutcome {
    /// The page was written to `meta.json`.
    Saved,
    /// The page is held in memory; it is written on a later turn or on close.
    Pending,
}

/// Record the page the reader is on. The session remembers it at once, and `meta.json` is
/// written at most once a second per book (every ten seconds after a failure). The rest is
/// written on close. A failed write is an error; the UI clears its warning only on `Saved`.
#[tauri::command]
pub async fn save_progress(
    app: AppHandle,
    root: String,
    page: u32,
    seq: u64,
) -> Result<SaveOutcome, CommandError> {
    blocking(move || {
        let state = app.state::<Mutex<Session>>();
        save_in(
            &state,
            Path::new(&root),
            page,
            seq,
            Instant::now(),
            |root, page| save_last_read_page(root, page).map(|_| ()),
        )
    })
    .await
}

/// The body of `save_progress`, with the session and the writer passed in so it can be
/// tested without a Tauri runtime. `seq` orders the webview's saves: a stale one is ignored.
fn save_in(
    session: &Mutex<Session>,
    root: &Path,
    page: u32,
    seq: u64,
    now: Instant,
    write: impl FnMut(&Path, u32) -> Result<(), crate::library::BookError>,
) -> Result<SaveOutcome, CommandError> {
    let root = canonical_key(root)?;
    let mut session = lock(session);
    if !session.record_seq(&root, page, seq, now)? {
        return Ok(SaveOutcome::Pending);
    }
    // The lock is held across the write, as in `flush_session`, so writers never interleave.
    let failures = session.flush(Some(&root), now, write);
    match failures.into_iter().next() {
        Some((root, err)) => {
            eprintln!(
                "could not save reading position for {}: {err}",
                root.display()
            );
            Err(err.into())
        }
        None => Ok(SaveOutcome::Saved),
    }
}

/// Reopen the book recorded in `state.json`. Returns `None`, never an error, when there is
/// no such record or the book no longer loads.
#[tauri::command]
pub async fn last_book(app: AppHandle) -> Result<Option<BookView>, CommandError> {
    blocking(move || {
        let Some(root) = read_last_book(&app.state::<Paths>().state_file) else {
            return Ok(None);
        };
        // The home screen is the fallback, but say why the book did not reopen.
        Ok(open_and_register(&app, &root)
            .inspect_err(|e| eprintln!("could not reopen {}: {}", root.display(), e.message))
            .ok())
    })
    .await
}
