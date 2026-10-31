// Types that match the JSON the Rust commands send. Keep them in step with
// `src-tauri/src/commands/view.rs`, `library/inspect.rs`, and `library/command_error.rs`.

/** A Scriptorium book package that loaded cleanly. */
export interface BookFolderInfo {
  kind: "book";
  id: string;
  title: string;
  pages: number;
}

/** A folder of scans that can become a book. */
export interface ScansFolderInfo {
  kind: "scans";
  /** The number of supported image files. */
  files: number;
  /** The number of pages the book would have: all frames of all files. */
  frames: number;
  /** Bytes the import will write. */
  estimatedBytes: number;
  /** An id that passes `isValidBookId` and is free in the library. */
  suggestedId: string;
  /** The folder name. */
  suggestedTitle: string;
}

/** What `inspect_folder` says a picked folder holds. */
export type FolderInfo = BookFolderInfo | ScansFolderInfo;

/** One page of an open book. */
export interface PageView {
  /** Canonical absolute path of the display image. Pass it to `pageUrl`. */
  path: string;
  width: number;
  height: number;
}

/** An open book. */
export interface BookView {
  id: string;
  title: string;
  /** Canonical absolute path of the package. Pass it back to `save_progress`. */
  root: string;
  /** Zero-based page to open on. */
  lastReadPage: number;
  /** The highest save number Rust has for this book. New saves are numbered above it. */
  saveSeq: number;
  pages: PageView[];
}

/** Payload of the `ingest-progress` event. */
export interface IngestProgress {
  /** The `importId` given to `create_book`. Ignore ticks for any other id. */
  importId: string;
  done: number;
  total: number;
}

/**
 * The error every command rejects with. `kind` is a `BookError` variant name, "Internal",
 * or "SaveHeld" (a page held after an earlier failed save).
 */
export interface CommandError {
  kind: string;
  message: string;
}

/** Arguments of the `create_book` command. */
export interface CreateBookArgs {
  folder: string;
  title: string;
  id: string;
  rights: string;
  attribution: string;
  /** This import's own id, echoed in every `ingest-progress` tick. See `newImportId`. */
  importId: string;
}

/** Payload of the `progress-saved` event: a background write put `root`'s page on disk. */
export interface ProgressSaved {
  /** The `root` of the open `BookView`. */
  root: string;
}

/** What `save_progress` did: wrote the page, or held it for a later write. */
export type SaveOutcome = "saved" | "pending";
