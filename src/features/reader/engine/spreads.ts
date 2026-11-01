// Spread model (ADR-0001). Slot 0 is a blank front pastedown, scan p is slot p + 1, and a blank
// back pastedown pads the slot count to even. Spread s shows slots 2s and 2s + 1. Pure functions.

/** The spread that shows scan p (0-based). */
export function spreadOfPage(p: number): number {
  return Math.floor((p + 1) / 2);
}

/** The first scan visible in spread s. Spread 0 shows scan 0 alone, on the right. */
export function firstPageOfSpread(s: number): number {
  return Math.max(0, 2 * s - 1);
}

/** How many spreads a book of pageCount scans has. A book with no pages has none. */
export function spreadCount(pageCount: number): number {
  return pageCount <= 0 ? 0 : Math.floor(pageCount / 2) + 1;
}

/**
 * The scans visible in spread s: none for an out-of-range spread, one for the first spread or a
 * last spread that pairs one scan with the back pastedown, otherwise two.
 */
export function pagesOfSpread(s: number, pageCount: number): number[] {
  if (!Number.isInteger(s) || s < 0 || s >= spreadCount(pageCount)) return [];
  return [2 * s - 1, 2 * s].filter((p) => p >= 0 && p < pageCount);
}

/** The scan shown in a slot, or null for a pastedown or an out-of-range slot. */
export function pageOfSlot(slot: number, pageCount: number): number | null {
  const p = slot - 1;
  return Number.isInteger(p) && p >= 0 && p < pageCount ? p : null;
}
