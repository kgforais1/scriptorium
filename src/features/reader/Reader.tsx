import { useCallback, useRef, useState } from "react";
import lamplitRaw from "../../../design/skins/lamplit/scene.json";
import type { BookView } from "../../lib/bookTypes";
import { parseScene, spreadCount, spreadOfPage, type EngineChange, type Scene } from "./engine";
import { canvasLabel, pageIndicator, pageRange } from "./labels";
import { NotesPanel } from "./NotesPanel";
import { ReaderBar } from "./ReaderBar";
import { ReaderHeader } from "./ReaderHeader";
import { ReaderStatus } from "./ReaderStatus";
import { useBookSession } from "./useBookSession";
import { useEngine } from "./useEngine";
import { useEscape, usePageKeys } from "./useReaderKeys";
import { useReducedMotion } from "./useReducedMotion";
import "./reader.css";

type Props = {
  book: BookView;
  /** Called once the book is closed and its last page is saved. */
  onClose: () => void;
  /** The scene to draw. Defaults to the lamplit scene. */
  scene?: Scene;
};

/**
 * The reader: the flipbook canvas with its chrome. Give it a `key` per book. It saves the first
 * scan of each spread, shows a failed save in the bottom bar, and closes through **Close book**.
 */
export function Reader({ book, onClose, scene }: Readonly<Props>) {
  const [defaultScene] = useState(() => parseScene(lamplitRaw));
  const pageCount = book.pages.length;
  const openedSpread = spreadOfPage(book.lastReadPage);
  const reduced = useReducedMotion();
  const [change, setChange] = useState<EngineChange | null>(null);
  const [scrub, setScrub] = useState<number | null>(null);
  const [pageNote, setPageNote] = useState<string | null>(null);
  const [notesOpen, setNotesOpen] = useState(false);
  const session = useBookSession(book.root, openedSpread, onClose);
  const { save } = session;

  const seenSpread = useRef(openedSpread);

  const onChange = useCallback(
    (next: EngineChange) => {
      setChange(next);
      // The scrubber follows the engine again once the engine settles, wherever it landed.
      if (!next.turning) setScrub(null);
      if (next.spread !== seenSpread.current) {
        seenSpread.current = next.spread;
        setPageNote(null);
      }
      save(next.spread);
    },
    [save],
  );

  const { canvasRef, controllerRef, status, error } = useEngine({
    book,
    scene: scene ?? defaultScene,
    reduced,
    onChange,
    onPageError: (e) => setPageNote(e.page === null ? e.message : `Page ${e.page + 1} could not be loaded.`),
  });

  const spreads = change?.spreads ?? spreadCount(pageCount);
  const shown = scrub ?? change?.spread ?? openedSpread;
  const ready = status === "ready" && change !== null;
  const turning = change?.turning ?? false;

  usePageKeys(controllerRef, spreads);
  const closeNotes = useCallback(() => setNotesOpen(false), []);
  useEscape(notesOpen, closeNotes);

  const indicator = pageIndicator(shown, pageCount);

  return (
    <div className="reader" data-turning={turning} data-notes={notesOpen}>
      {pageCount > 0 && status !== "error" && (
        <canvas
          ref={canvasRef}
          className="reader__canvas"
          data-ready={ready}
          tabIndex={0}
          aria-label={canvasLabel(change?.spread ?? openedSpread, pageCount)}
        />
      )}
      <ReaderStatus empty={pageCount === 0} loading={status === "loading"} error={status === "error" ? error : null} />
      <ReaderHeader title={book.title} indicator={indicator} closing={session.closing} onClose={() => void session.close()} />
      <ReaderBar
        spread={shown}
        spreads={spreads}
        rangeLabel={pageRange(shown, pageCount)}
        indicator={indicator}
        enabled={ready}
        notesOpen={notesOpen}
        saveNote={session.saveError}
        pageNote={pageNote}
        onPrev={() => controllerRef.current?.prev()}
        onNext={() => controllerRef.current?.next()}
        onScrub={(s) => {
          // A step back to the settled spread starts no turn, so no change would clear the hold.
          setScrub(!turning && s === change?.spread ? null : s);
          controllerRef.current?.goTo(s);
        }}
        onToggleNotes={() => setNotesOpen((open) => !open)}
      />
      <NotesPanel open={notesOpen} onClose={closeNotes} />
    </div>
  );
}
