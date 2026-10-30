/**
 * Tracks which save result the page bar may show. Saves can overlap, so a result older than
 * one already shown is ignored. Only results that change the note count: "saved" clears it
 * and an error sets it. "pending" says nothing about the disk, so it never advances the
 * marker, or a later "pending" could hide an earlier failure.
 *
 * @returns `next()` numbers a new save; `accept(call)` says whether its result may be shown.
 */
export function saveResultGate(): { next: () => number; accept: (call: number) => boolean } {
  let calls = 0;
  let shown = 0;
  return {
    next: () => ++calls,
    accept: (call) => {
      if (call < shown) {
        return false;
      }
      shown = call;
      return true;
    },
  };
}
