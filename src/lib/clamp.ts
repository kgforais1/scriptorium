/**
 * Clamps a page index into the valid range for a book.
 *
 * @param page - The requested zero-based page index.
 * @param count - The number of pages in the book.
 * @returns A page index in `[0, count - 1]`, or 0 when the book has no pages.
 */
export function clampPage(page: number, count: number): number {
  if (count <= 0) {
    return 0;
  }
  return Math.min(Math.max(page, 0), count - 1);
}
