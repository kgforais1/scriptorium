const BYTES_PER_MB = 1024 * 1024;

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
 * Builds the page indicator text.
 *
 * @param page - The zero-based current page.
 * @param count - The number of pages in the book.
 * @returns "n / total" with a one-based n. An empty book gives "0 / 0".
 */
export function pageLabel(page: number, count: number): string {
  return count <= 0 ? "0 / 0" : `${page + 1} / ${count}`;
}

/**
 * Builds the accessible text for the page image.
 *
 * @param page - The zero-based current page.
 * @param count - The number of pages in the book.
 * @returns Text such as "Page 3 of 40".
 */
export function pageAlt(page: number, count: number): string {
  return `Page ${page + 1} of ${count}`;
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
