import { useCallback, useRef, useState } from "react";
import type { EngineChange, EngineError } from "./engine";
import { failedPagesNote } from "./labels";

/**
 * The note about pages that did not load. A page is reported once and never retried, so failed
 * scans are kept by index and the note follows whichever spread is open. A whole-engine message
 * (a null page) belongs to the spread it arrived on and clears when the spread changes.
 *
 * @param pageCount - The number of scans in the book.
 * @param openedSpread - The spread the book opened on.
 * @param change - The engine's latest change, or null before the first one.
 * @returns The note text or null, and the two handlers to feed: `spreadChanged` with each engine
 * change, and `pageFailed` with each engine error.
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

  const pageNote = engineNote ?? failedPagesNote(failedPages, change?.spread ?? openedSpread, pageCount);
  return { pageNote, spreadChanged, pageFailed };
}
