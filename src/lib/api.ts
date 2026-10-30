import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  BookView,
  CreateBookArgs,
  FolderInfo,
  IngestProgress,
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
 * @param args - The source folder and the book's id, title, rights, and attribution.
 * @returns The open book. Rejects with a `CommandError`.
 */
export function createBook(args: CreateBookArgs): Promise<BookView> {
  const { folder, title, id, rights, attribution } = args;
  return call<BookView>("create_book", { folder, title, id, rights, attribution });
}

/**
 * Opens a book package in place.
 *
 * @param path - Absolute path of the package folder.
 * @returns The open book. Rejects with a `CommandError`.
 */
export function openBook(path: string): Promise<BookView> {
  return call<BookView>("open_book", { path });
}

// Numbers every save in this run. Rust ignores a save numbered below one it already has, so a
// slow older save can never overwrite a newer page, even across two views of one book.
let saveSeq = 0;

/**
 * Records the page the reader is on.
 *
 * @param root - The `root` of the open `BookView`.
 * @param page - The zero-based page.
 * @returns `"saved"` when the page reached `meta.json`, `"pending"` while it is held for a
 * later write. Rejects with a `CommandError` when a write fails.
 */
export function saveProgress(root: string, page: number): Promise<SaveOutcome> {
  saveSeq += 1;
  return call<SaveOutcome>("save_progress", { root, page, seq: saveSeq });
}

/**
 * Reopens the book that was open last time.
 *
 * @returns The book, or null when there is none or it no longer loads.
 */
export function lastBook(): Promise<BookView | null> {
  return call<BookView | null>("last_book");
}

/**
 * Listens for ingest progress while `createBook` runs.
 *
 * @param cb - Called with each progress tick.
 * @returns A function that stops listening.
 */
export function onIngestProgress(
  cb: (progress: IngestProgress) => void,
): Promise<() => void> {
  return guarded(() =>
    listen<IngestProgress>("ingest-progress", (event) => {
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
