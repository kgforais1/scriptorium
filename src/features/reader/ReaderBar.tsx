type Props = {
  /** The spread the scrubber shows, zero-based. */
  spread: number;
  spreads: number;
  /** Short live label beside the scrubber, such as "4–5". */
  rangeLabel: string;
  /** Full text for assistive technology, such as "pages 4–5 of 202". */
  indicator: string;
  enabled: boolean;
  notesOpen: boolean;
  /** The failed-save note, or null. */
  saveNote: string | null;
  /** A page that could not be loaded, or null. */
  pageNote: string | null;
  onPrev: () => void;
  onNext: () => void;
  onScrub: (spread: number) => void;
  onToggleNotes: () => void;
};

function Chevron({ direction }: Readonly<{ direction: "left" | "right" }>) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false" className="reader__icon">
      <path d={direction === "left" ? "M14.5 6l-6 6 6 6" : "M9.5 6l6 6-6 6"} />
    </svg>
  );
}

/** The floating bar at the bottom: Previous, a page scrubber, Next, the notes toggle, and notes. */
export function ReaderBar(props: Readonly<Props>) {
  const { spread, spreads, rangeLabel, indicator, enabled, notesOpen, saveNote, pageNote } = props;
  return (
    <nav className="reader__bar" aria-label="Page navigation">
      <button
        type="button"
        className="reader__button"
        aria-label="Previous page"
        disabled={!enabled || spread <= 0}
        onClick={props.onPrev}
      >
        <Chevron direction="left" />
      </button>
      <input
        type="range"
        className="reader__scrubber"
        aria-label="Go to page"
        aria-valuetext={indicator}
        min={0}
        max={Math.max(0, spreads - 1)}
        step={1}
        value={spread}
        disabled={!enabled || spreads <= 1}
        onChange={(e) => props.onScrub(Number(e.target.value))}
      />
      <output className="reader__range" aria-hidden="true">
        {rangeLabel}
      </output>
      <button
        type="button"
        className="reader__button"
        aria-label="Next page"
        disabled={!enabled || spread >= spreads - 1}
        onClick={props.onNext}
      >
        <Chevron direction="right" />
      </button>
      <span className="reader__sep" aria-hidden="true" />
      <button
        type="button"
        className="reader__button reader__button--text"
        aria-pressed={notesOpen}
        aria-controls="reader-notes"
        onClick={props.onToggleNotes}
      >
        Notes
      </button>
      {saveNote !== null && <output className="reader__note reader__note--error">Your place could not be saved. {saveNote}</output>}
      {pageNote !== null && <output className="reader__note">{pageNote}</output>}
    </nav>
  );
}
