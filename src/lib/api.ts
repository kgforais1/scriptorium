import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  BookView,
  CreateBookArgs,
  FolderInfo,
  IngestProgress,
  ProgressSaved,
  SaveOutcome,
} from "./bookTypes";
import { toCommandError } from "./errors";

/** Runs a command and turns any rejection into a `CommandError`. */
async function guarded<T>(work: () => Promise<T>): Promise<T> {
  try {
    return await work();
  } catch (err) {
    throw toCommandError(err);
  }
}

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return guarded(() => invoke<T>(cmd, args));
}

/**
 * Says what a folder holds: a Scriptorium book, or scans that could become one.
 *
 * @param path - Absolute path of the folder.
 * @returns The folder info. Rejects with a `CommandError`.
 */
export function inspectFolder(path: string): Promise<FolderInfo> {
  return call<FolderInfo>("inspect_folder", { path });
}

/**
 * Builds a book from a folder of scans and opens it.
 *
 * @param args - The source folder, the book's id, title, rights, and attribution, and the
 * import id that tags this import's progress ticks (see `newImportId`).
 * @returns The open book. Rejects with a `CommandError`.
 */
export function createBook(args: CreateBookArgs): Promise<BookView> {
  const { folder, title, id, rights, attribution, importId } = args;
  return call<BookView>("create_book", {
    folder,
    title,
    id,
    rights,
    attribution,
    importId,
  }).then(seeded);
}

/**
 * Opens a book package in place.
 *
 * @param path - Absolute path of the package folder.
 * @returns The open book. Rejects with a `CommandError`.
 */
export function openBook(path: string): Promise<BookView> {
  return call<BookView>("open_book", { path }).then(seeded);
}

// Numbers every save in this run. Rust ignores a save numbered below one it already has, so a
// slow older save can never overwrite a newer page, even across two views of one book.
let saveSeq = 0;
// The latest number issued per book root, so a background write can be compared with it.
const issuedSeq = new Map<string, number>();

/** Numbers later saves above the book's own, so a reloaded webview is never "stale". */
function seeded(book: BookView): BookView {
  saveSeq = Math.max(saveSeq, book.saveSeq);
  return book;
}

/**
 * Records the page the reader is on.
 *
 * @param root - The `root` of the open `BookView`.
 * @param page - The zero-based page.
 * @returns `"saved"` when the page reached `meta.json`, `"pending"` while it is held for a
 * later write. Rejects with a `CommandError` when a write fails, or with kind "SaveHeld" when
 * the page is held after an earlier failure.
 */
export function saveProgress(root: string, page: number): Promise<SaveOutcome> {
  saveSeq += 1;
  issuedSeq.set(root, saveSeq);
  return call<SaveOutcome>("save_progress", { root, page, seq: saveSeq });
}

/**
 * The number of the latest `saveProgress` call for a book. Read it right after a call to get
 * that call's number. Compare it with `ProgressSaved.seq` to tell whether a background write
 * covers every save issued so far.
 *
 * @param root - The `root` of the open `BookView`.
 * @returns The latest save number issued for `root`, or 0 when none was issued.
 */
export function latestSaveSeq(root: string): number {
  return issuedSeq.get(root) ?? 0;
}

/**
 * Writes the book's held page now, as the reader closes it.
 *
 * @param root - The `root` of the open `BookView`.
 * @returns Nothing. Rejects with a `CommandError` when the write fails.
 */
export function closeBook(root: string): Promise<void> {
  return call<void>("close_book", { root });
}

/**
 * Reopens the book that was open last time.
 *
 * @returns The book, or null when there is none or it no longer loads.
 */
export function lastBook(): Promise<BookView | null> {
  return call<BookView | null>("last_book").then((book) => (book ? seeded(book) : null));
}

let importCount = 0;
// Fixed per module load, so a reload that restarts the counter inside the same millisecond still
// gets different ids.
const loadNonce = crypto.randomUUID().slice(0, 8);

/**
 * Makes an id for one import, to pass to `createBook` and `onIngestProgress`. Ids differ
 * within a run and across reloads of the webview.
 *
 * @returns A fresh id.
 */
export function newImportId(): string {
  importCount += 1;
  return `${Date.now().toString(36)}-${loadNonce}-${importCount}`;
}

/**
 * Listens for the progress of one import while `createBook` runs. Ticks of other imports are
 * ignored.
 *
 * @param importId - The id passed to `createBook`.
 * @param cb - Called with each progress tick of that import.
 * @returns A function that stops listening.
 */
export function onIngestProgress(
  importId: string,
  cb: (progress: IngestProgress) => void,
): Promise<() => void> {
  return guarded(() =>
    listen<IngestProgress>("ingest-progress", (event) => {
      if (event.payload.importId === importId) {
        cb(event.payload);
      }
    }),
  );
}

/**
 * Listens for background saves of the reading position. Rust emits one when a held page
 * reaches `meta.json` without another page turn.
 *
 * @param cb - Called with the `root` of the book that was saved and the `seq` that was written.
 * @returns A function that stops listening.
 */
export function onProgressSaved(cb: (saved: ProgressSaved) => void): Promise<() => void> {
  return guarded(() =>
    listen<ProgressSaved>("progress-saved", (event) => {
      cb(event.payload);
    }),
  );
}

/**
 * Shows the system folder picker.
 *
 * @returns The chosen folder, or null when the user cancels.
 */
export function pickFolder(): Promise<string | null> {
  return guarded(() => open({ directory: true, multiple: false }));
}

/**
 * Turns a page path from a `BookView` into a URL an `<img>` can load.
 *
 * @param path - `PageView.path`.
 * @returns The asset-protocol URL.
 */
export function pageUrl(path: string): string {
  return convertFileSrc(path);
}
