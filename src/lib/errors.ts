import type { CommandError } from "./bookTypes";

const HEADINGS: Record<string, string> = {
  EmptyFolder: "Folder is empty",
  NoSupportedImages: "No supported images",
  UnreadableFile: "Can't read a file",
  DestinationExists: "Book already exists",
  DuplicatePageStem: "Two files share a name",
  InsufficientDiskSpace: "Not enough disk space",
  UnsupportedFormatVersion: "Book is from a newer version",
  InvalidPackage: "Not a valid book",
  ChecksumMismatch: "Book is damaged",
  PermissionDenied: "Access denied",
  Internal: "Scriptorium hit a problem",
};

const FALLBACK_HEADING = "Something went wrong";

/**
 * Gives a short heading for an error kind.
 *
 * @param kind - A `BookError` variant name, "Internal", or anything else.
 * @returns A heading for the error dialog. Unknown kinds get a generic one.
 */
export function errorHeading(kind: string): string {
  return Object.prototype.hasOwnProperty.call(HEADINGS, kind) ? HEADINGS[kind] : FALLBACK_HEADING;
}

/**
 * Tells whether a value has the shape of a `CommandError`.
 *
 * @param value - Anything a command might reject with.
 * @returns True when `value` is an object with string `kind` and `message`.
 */
export function isCommandError(value: unknown): value is CommandError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const { kind, message } = value as Record<string, unknown>;
  return typeof kind === "string" && typeof message === "string";
}

/**
 * Turns anything a command rejected with into a `CommandError`.
 *
 * @param err - The rejection value.
 * @returns `err` when it already has the shape. Otherwise kind "Unknown" with `String(err)`.
 */
export function toCommandError(err: unknown): CommandError {
  if (isCommandError(err)) {
    return err;
  }
  return { kind: "Unknown", message: String(err) };
}
