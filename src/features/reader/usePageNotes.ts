import { useCallback, useRef, useState } from "react";
import type { EngineChange, EngineError } from "./engine";
import { failedPagesNote } from "./labels";

/**
 * The failed set without a page that has now loaded. Returns the same set when the page was not
 * in it, so React skips the re-render on the common path (every successful load).
 *
 * @param failed - The pages that failed so far.
 * @param page - The page that loaded.
 * @returns The set without that page.
 */
export function withoutPage(failed: ReadonlySet<number>, page: number): ReadonlySet<number> {
  if (!failed.has(page)) return failed;
  const next = new Set(failed);
  next.delete(page);
  return next;
}

/**
 * The note about pages that did not load. A page is reported once and never retried, so failed
 * scans are kept by index and the note follows whichever spread is open. A whole-engine message
 * (a null page) belongs to the spread it arrived on and clears when the spread changes, or when a
 * later resize, scene change, or context restore succeeds. A failed page leaves the note once it
 * loads after all (after a context restore, failed pages are tried again).
 *
 * @param pageCount - The number of scans in the book.
 * @param openedSpread - The spread the book opened on.
 * @param change - The engine's latest change, or null before the first one.
 * @returns The note text or null, and the handlers to feed: `spreadChanged` with each engine
 * change, `pageFailed` with each engine error, and `recovered` with each engine recovery.
 */
export function usePageNotes(pageCount: number, openedSpread: number, change: EngineChange | null) {
  const [failedPages, setFailedPages] = useState<ReadonlySet<number>>(() => new Set());
  const [engineNote, setEngineNote] = useState<string | null>(null);
  const seenSpread = useRef(openedSpread);

  const spreadChanged = useCallback((next: number) => {
    if (next === seenSpread.current) return;
    seenSpread.current = next;
    setEngineNote(null);
  }, []);

  const pageFailed = useCallback((e: EngineError) => {
    const page = e.page;
    if (page === null) setEngineNote(e.message);
    else setFailedPages((prev) => new Set(prev).add(page));
  }, []);

  const recovered = useCallback((page: number | null) => {
    if (page === null) {
      setEngineNote(null);
      return;
    }
    setFailedPages((prev) => withoutPage(prev, page));
  }, []);

  const pageNote = engineNote ?? failedPagesNote(failedPages, change?.spread ?? openedSpread, pageCount);
  return { pageNote, spreadChanged, pageFailed, recovered };
}
