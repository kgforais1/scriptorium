//! Tauri commands: the only way the webview touches books.
//!
//! This file is thin glue. The logic that can be tested without a Tauri runtime lives in
//! [`session`] (debounce, background-write claims and close flush), [`state_file`] (`state.json`), [`throttle`]
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
use crate::library::load::{load_book, LoadedBook};
use crate::library::progress::save_last_read_page;
use crate::library::sweep::{sweep_orphans, ORPHAN_AGE};
use crate::library::BookError;
use serde::Serialize;
use session::{canonical_key, Session, WorkerStep, SAVE_INTERVAL};
use state_file::{read_last_book, write_last_book};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
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
#[serde(rename_all = "camelCase")]
struct IngestProgress {
    /// The id the webview gave `create_book`, so it can ignore another import's ticks.
    import_id: String,
    done: u32,
    total: u32,
}

/// Payload of the `progress-saved` event: a background write put `root`'s page on disk.
#[derive(Debug, Clone, Serialize)]
struct ProgressSaved {
    /// The book's root as the view spells it (`BookView.root`).
    root: String,
    /// The webview save sequence of the page that was written. A save numbered above this is
    /// newer than the write, so the webview must not treat it as covered.
    seq: u64,
}

impl ProgressSaved {
    /// The payload for a write of `seq` to the book at `root`.
    fn new(root: &Path, seq: u64) -> Self {
        Self {
            root: dunce::simplified(root).to_string_lossy().into_owned(),
            seq,
        }
    }
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
    register_in_session(
        &mut lock(&app.state::<Mutex<Session>>()),
        &book,
        &mut view,
        Instant::now(),
    );
    // Failing to remember the book must not fail the open.
    if let Err(e) = write_last_book(&app.state::<Paths>().state_file, &book.root) {
        eprintln!("could not write state.json: {e}");
    }
    Ok(view)
}

/// Track `book` in `session` and fill in the parts of `view` that depend on it: the page to
/// open on (an unsaved page from an earlier view wins over the one on disk) and the highest
/// save number so far, so a reopened view numbers its saves above it.
fn register_in_session(
    session: &mut Session,
    book: &LoadedBook,
    view: &mut BookView,
    now: Instant,
) {
    let page_count = u32::try_from(book.meta.pages.len()).unwrap_or(u32::MAX);
    view.last_read_page =
        session.register(book.root.clone(), book.meta.last_read_page, page_count, now);
    view.save_seq = session.last_seq(&book.root);
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
/// `{ importId, done, total }` at most about ten times a second, and always once at
/// `done == total`. `import_id` is the webview's own id for this import, echoed in every tick.
#[tauri::command]
pub async fn create_book(
    app: AppHandle,
    folder: String,
    title: String,
    id: String,
    rights: String,
    attribution: String,
    import_id: String,
) -> Result<BookView, CommandError> {
    let opts = CreateOptions {
        id,
        title,
        rights,
        attribution,
    };
    blocking(move || create_and_open(&app, Path::new(&folder), &opts, &import_id)).await
}

/// The blocking body of `create_book`: build the package, then open it. Runs on the blocking
/// pool, never on the async runtime.
fn create_and_open(
    app: &AppHandle,
    folder: &Path,
    opts: &CreateOptions,
    import_id: &str,
) -> Result<BookView, CommandError> {
    let mut throttle = EmitThrottle::new(EMIT_INTERVAL);
    let library = app.state::<Paths>().library.clone();
    let dest = create_book_from_folder_with(folder, &library, opts, |done, total| {
        if throttle.should_emit(done, total, Instant::now()) {
            // A webview that went away is not an ingest failure.
            let tick = IngestProgress {
                import_id: import_id.to_owned(),
                done,
                total,
            };
            let _ = app.emit("ingest-progress", tick);
        }
    })?;
    // A book that was built but cannot be opened would block a retry with the same id, so
    // remove it before reporting the error.
    open_and_register(app, &dest).inspect_err(|_| {
        if let Err(e) = std::fs::remove_dir_all(&dest) {
            eprintln!("could not remove {}: {e}", dest.display());
        }
    })
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
    /// The page is held in memory; a background worker writes it once it is due, or close does.
    Pending,
}

/// Record the page the reader is on. The session remembers it at once, and `meta.json` is
/// written at most once a second per book (every ten seconds after a failure). A page that is
/// not written yet is written by one background worker per book, which emits `progress-saved`
/// when it succeeds; close writes whatever is left. A failed write is an error, and so is a
/// page held after an earlier failure (`SaveHeld`). The UI clears its warning on `Saved` and
/// on `progress-saved`.
#[tauri::command]
pub async fn save_progress(
    app: AppHandle,
    root: String,
    page: u32,
    seq: u64,
) -> Result<SaveOutcome, CommandError> {
    blocking(move || {
        let state = app.state::<Mutex<Session>>();
        let result = save_in(
            &state,
            Path::new(&root),
            page,
            seq,
            Instant::now(),
            |r, p| save_last_read_page(r, p).map(|_| ()),
        );
        // Anything not yet on disk gets one background worker (no-op if one runs).
        if let Some((key, token)) = claim_after_save(&state, Path::new(&root), &result) {
            spawn_worker(&app, key, token);
        }
        result
    })
    .await
}

/// After a save that did not reach disk (held, or failed), claim the book's background write.
/// Returns the book's key and the worker's token, or `None` when the page was written, the
/// book is clean or unknown, or a worker already owns the write. Callers start exactly one
/// worker per `Some`.
fn claim_after_save(
    session: &Mutex<Session>,
    root: &Path,
    result: &Result<SaveOutcome, CommandError>,
) -> Option<(PathBuf, u64)> {
    if *result == Ok(SaveOutcome::Saved) {
        return None;
    }
    let key = canonical_key(root).ok()?;
    let token = lock(session).claim_worker(&key)?;
    Some((key, token))
}

/// Start the thread that owns one book's background write. It sleeps until the write is due,
/// writes it, and on success tells the webview with `progress-saved`.
fn spawn_worker(app: &AppHandle, root: PathBuf, token: u64) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<Mutex<Session>>() else {
            return;
        };
        let announce = |seq: u64| {
            // A webview that went away is not a save failure.
            let _ = app.emit("progress-saved", ProgressSaved::new(&root, seq));
        };
        run_worker(
            &state,
            &root,
            token,
            |r, p| save_last_read_page(r, p).map(|_| ()),
            std::thread::sleep,
            announce,
        );
    });
}

/// The loop of a background worker: wait until the book's write is due, write it under the
/// session lock, and stop once the book is saved, clean, gone, or claimed by another worker.
/// A failed write keeps the worker looping at the retry backoff. `on_saved` runs once, after
/// the lock is released, when this worker's write succeeded. It gets the save sequence written. The writer and the sleep are
/// passed in so the loop is testable without a Tauri runtime.
fn run_worker(
    session: &Mutex<Session>,
    root: &Path,
    token: u64,
    mut write: impl FnMut(&Path, u32) -> Result<(), BookError>,
    sleep: impl Fn(Duration),
    on_saved: impl FnOnce(u64),
) {
    loop {
        let step = lock(session).worker_step(root, token, Instant::now(), &mut write);
        match step {
            WorkerStep::Wait(due) => sleep(due.saturating_duration_since(Instant::now())),
            WorkerStep::Failed(err) => eprintln!(
                "could not save reading position for {}: {err}",
                root.display()
            ),
            WorkerStep::Saved(seq) => return on_saved(seq),
            WorkerStep::Done => return,
        }
    }
}

/// Write the book's held page now. Called when the reader closes the book.
#[tauri::command]
pub async fn close_book(app: AppHandle, root: String) -> Result<(), CommandError> {
    blocking(move || {
        let root = canonical_key(Path::new(&root))?;
        flush_session(&app, Some(&root)).map_or(Ok(()), Err)
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
        // A held page after a failed write is not safe yet: say so, so the warning stays up.
        if session.write_failed(&root) {
            return Err(CommandError::save_held());
        }
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
