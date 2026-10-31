/**
 * Tracks which save result the page bar may show. Results are keyed by the save's sequence
 * number (the one `saveProgress` sends), which only grows. Saves can overlap, so a result older
 * than one already shown is ignored. Only results that change the note count: "saved" clears it
 * and an error sets it. "pending" says nothing about the disk, so it never advances the
 * marker, or a later "pending" could hide an earlier failure.
 *
 * A background save (`progress-saved`) puts the page numbered `seq` on disk. It settles the
 * saves up to that number: their results, which may still be in flight, are stale. A save
 * issued after it is not covered, so its own result still counts.
 *
 * @returns `accept(seq)` says whether the result of save `seq` may be shown; `settle(written,
 * latest)` marks saves up to `written` as stale and says whether the note may clear, which is
 * so only when `written` reaches `latest`, the newest save issued.
 */
export function saveResultGate(): {
  accept: (seq: number) => boolean;
  settle: (written: number, latest: number) => boolean;
} {
  let shown = 0;
  return {
    accept: (seq) => {
      if (seq < shown) {
        return false;
      }
      shown = seq;
      return true;
    },
    settle: (written, latest) => {
      shown = Math.max(shown, written + 1);
      return written >= latest;
    },
  };
}
