const BYTES_PER_MB = 1_000_000;

/**
 * Formats a byte count for the import confirmation.
 *
 * @param bytes - A size in bytes.
 * @returns The size in megabytes with one decimal, such as "12.5 MB". Negative or
 * non-finite input gives "0.0 MB".
 */
export function formatMegabytes(bytes: number): string {
  const safe = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  return `${(safe / BYTES_PER_MB).toFixed(1)} MB`;
}

/**
 * Builds the progress text shown while a book is imported.
 *
 * @param done - Pages written so far.
 * @param total - Pages to write.
 * @returns Text such as "12 / 40 pages".
 */
export function progressLabel(done: number, total: number): string {
  return `${done} / ${total} pages`;
}
