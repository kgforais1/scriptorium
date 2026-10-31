import { useEffect, useRef } from "react";

type Props = { open: boolean; onClose: () => void };

/** The notes panel: an empty shell until hotspots arrive in M6. Closed, it is inert. Escape closes it from the reader's key handler. */
export function NotesPanel({ open, onClose }: Readonly<Props>) {
  const panelRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const wasOpen = useRef(open);

  // Opening moves focus into the panel. Closing returns it to the Notes toggle if it was inside,
  // because an inert panel would otherwise drop it on the body.
  useEffect(() => {
    if (open === wasOpen.current) return;
    wasOpen.current = open;
    if (open) closeRef.current?.focus();
    else if (panelRef.current?.contains(document.activeElement) || document.activeElement === document.body) {
      document.querySelector<HTMLElement>('[aria-controls="reader-notes"]')?.focus();
    }
  }, [open]);

  return (
    <aside
      id="reader-notes"
      ref={panelRef}
      className="reader__panel"
      aria-label="Notes"
      data-open={open}
      inert={!open}
    >
      <div className="reader__panel-head">
        <h2 className="reader__kicker">Notes</h2>
        <button ref={closeRef} type="button" className="reader__button" aria-label="Close notes" onClick={onClose}>
          <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false" className="reader__icon">
            <path d="M6 6l12 12M18 6L6 18" />
          </svg>
        </button>
      </div>
      <p className="reader__panel-empty">This book has no notes yet.</p>
    </aside>
  );
}
