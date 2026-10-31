import { firstPageOfSpread, pagesOfSpread } from "./engine/spreads";

/**
 * The page numbers a spread shows, one-based, for the scrubber label.
 *
 * @param spread - The zero-based spread.
 * @param pageCount - The number of scans in the book.
 * @returns "1" for a single page, "4–5" for a pair, and "" for a book with no such spread.
 */
export function pageRange(spread: number, pageCount: number): string {
  const pages = pagesOfSpread(spread, pageCount);
  if (pages.length === 0) return "";
  if (pages.length === 1) return String(pages[0] + 1);
  return `${pages[0] + 1}–${pages[pages.length - 1] + 1}`;
}

/**
 * The page indicator in the header.
 *
 * @param spread - The zero-based spread.
 * @param pageCount - The number of scans in the book.
 * @returns "page 1 of 202" for a single page, "pages 4–5 of 202" for a pair, and "no pages" for
 * an empty book.
 */
export function pageIndicator(spread: number, pageCount: number): string {
  const range = pageRange(spread, pageCount);
  if (range === "") return "no pages";
  const noun = pagesOfSpread(spread, pageCount).length > 1 ? "pages" : "page";
  return `${noun} ${range} of ${pageCount}`;
}

/**
 * The accessible name of the canvas, which announces the open pages.
 *
 * @param spread - The zero-based spread.
 * @param pageCount - The number of scans in the book.
 * @returns Text such as "Open book, pages 4–5 of 202".
 */
export function canvasLabel(spread: number, pageCount: number): string {
  return `Open book, ${pageIndicator(spread, pageCount)}`;
}

/**
 * Which page to save after the reader reaches a spread.
 *
 * @param savedSpread - The spread the last save (or the opening) was for.
 * @param spread - The spread now showing.
 * @returns The scan index to save, `firstPageOfSpread(spread)`, or null when the spread did not
 * change. Progress is always a scan index, never a spread index, so it survives a moved library.
 */
export function pageToSave(savedSpread: number, spread: number): number | null {
  return spread === savedSpread ? null : firstPageOfSpread(spread);
}

/**
 * The note for pages of the open spread that failed to load.
 *
 * @param failed - The zero-based scan indexes that failed, over the whole book.
 * @param spread - The zero-based spread on show.
 * @param pageCount - The number of scans in the book.
 * @returns "Page 7 could not be loaded." or "Pages 6 and 7 could not be loaded.", or null when
 * no page of the spread failed.
 */
export function failedPagesNote(failed: ReadonlySet<number>, spread: number, pageCount: number): string | null {
  const bad = pagesOfSpread(spread, pageCount).filter((p) => failed.has(p));
  if (bad.length === 0) return null;
  if (bad.length === 1) return `Page ${bad[0] + 1} could not be loaded.`;
  return `Pages ${bad.map((p) => p + 1).join(" and ")} could not be loaded.`;
}
