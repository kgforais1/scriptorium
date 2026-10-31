import { useCallback, useEffect, useRef, useState } from "react";
import { closeBook, latestSaveSeq, onProgressSaved, saveProgress } from "../../lib/api";
import { toCommandError } from "../../lib/errors";
import { saveResultGate } from "../../lib/saveResult";
import { pageToSave } from "./labels";

/**
 * The save side of an open book, carried over from Phase 2's `BookView`. `save(spread)` writes
 * the first scan of the spread, and a failed save sets a note instead of a dialog. A `saved`
 * outcome clears the note. So does a `progress-saved` event for this book whose write covers the newest save issued. `close()` awaits
 * `closeBook`, stays disabled while it runs, and on failure leaves the book open with the note.
 *
 * @param root - The `root` of the open book.
 * @param openedSpread - The spread the book opened on, which needs no save.
 * @param onClosed - Called once the final write succeeded.
 * @returns The note text, whether a close is running, and the two actions.
 */
export function useBookSession(root: string, openedSpread: number, onClosed: () => void) {
  const [saveError, setSaveError] = useState<string | null>(null);
  const [closing, setClosing] = useState(false);
  // Results of pending saves never go through the gate, so they cannot hide an earlier failure.
  const [gate] = useState(saveResultGate);
  const savedSpread = useRef(openedSpread);
  // Set at once, so a double click before the button disables starts one close, not two.
  const closingNow = useRef(false);

  // A background write puts its page on disk. If that is the latest page, the note no longer
  // applies. Saves it covers are older than the write, so the gate drops their results.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let stopped = false;
    onProgressSaved((event) => {
      // Only a write that reaches the newest save issued proves the page on disk is current.
      // An older write settles the saves it covers but leaves a newer save's result to speak.
      if (event.root === root && gate.settle(event.seq, latestSaveSeq(root))) setSaveError(null);
    }).then(
      (stop) => {
        if (stopped) stop();
        else unlisten = stop;
      },
      // The note then clears on the next successful turn instead.
      () => undefined,
    );
    return () => {
      stopped = true;
      unlisten?.();
    };
  }, [root, gate]);

  const save = useCallback(
    (spread: number) => {
      const page = pageToSave(savedSpread.current, spread);
      if (page === null) return;
      savedSpread.current = spread;
      // Each save is numbered, so Rust drops one that arrives after a newer page. Results can
      // overlap, so only one at least as new as the last shown may change the note.
      const pending = saveProgress(root, page);
      // The call numbered itself synchronously, so this is its own number.
      const call = latestSaveSeq(root);
      pending.then(
        (outcome) => {
          // Only a real write clears the note. A held page says nothing about the disk.
          if (outcome === "saved" && gate.accept(call)) setSaveError(null);
        },
        (err: unknown) => {
          if (gate.accept(call)) setSaveError(toCommandError(err).message);
        },
      );
    },
    [root, gate],
  );

  const close = useCallback(async () => {
    if (closingNow.current) return;
    closingNow.current = true;
    setClosing(true);
    try {
      await closeBook(root);
    } catch (err) {
      closingNow.current = false;
      setSaveError(toCommandError(err).message);
      setClosing(false);
      return;
    }
    onClosed();
  }, [root, onClosed]);

  return { saveError, closing, save, close };
}
