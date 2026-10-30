import { clampPage } from "./clamp";

/**
 * Works out the page after an arrow key.
 *
 * @param current - The current zero-based page.
 * @param count - The number of pages in the book.
 * @param key - A `KeyboardEvent.key` value.
 * @returns The previous page for "ArrowLeft", the next for "ArrowRight" (both stay in
 * range), and `current` for any other key.
 */
export function stepPage(current: number, count: number, key: string): number {
  if (key === "ArrowLeft") {
    return clampPage(current - 1, count);
  }
  if (key === "ArrowRight") {
    return clampPage(current + 1, count);
  }
  return current;
}
