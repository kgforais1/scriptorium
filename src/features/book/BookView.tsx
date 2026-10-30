import { useEffect, useRef, useState } from "react";
import { pageUrl, saveProgress } from "../../lib/api";
import type { BookView as Book } from "../../lib/bookTypes";
import { clampPage } from "../../lib/clamp";
import { isTypingTarget } from "../../lib/dom";
import { toCommandError } from "../../lib/errors";
import { pageAlt, pageLabel } from "../../lib/format";
import { stepPage } from "../../lib/paging";
import "./book.css";

interface BookViewProps {
  book: Book;
  /** Called when the user closes the book. */
  onClose: () => void;
}

/** Whether a key press should leave the page alone: a modifier is held, or a field or dialog has the focus. */
function shouldIgnoreKey(event: KeyboardEvent): boolean {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
    return true;
  }
  return isTypingTarget(event.target) || document.querySelector("dialog[open]") !== null;
}

/** Shows one page of an open book, with the page indicator, Prev/Next buttons, and arrow keys. */
export function BookView({ book, onClose }: Readonly<BookViewProps>) {
  const count = book.pages.length;
  const [page, setPage] = useState(() => clampPage(book.lastReadPage, count));
  // The path of a page image that failed to load, and its retry count (to force a reload).
  const [failedPath, setFailedPath] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  // A failed progress save is shown in the page bar, not a dialog, so a read-only book
  // stays readable.
  const [saveError, setSaveError] = useState<string | null>(null);

  function goTo(next: number) {
    if (next === page) {
      return;
    }
    setPage(next);
    // Each save is numbered, so Rust drops one that arrives after a newer page.
    saveProgress(book.root, next).then(
      (outcome) => {
        // Only a real write clears the warning; a held page says nothing about the disk.
        if (outcome === "saved") {
          setSaveError(null);
        }
      },
      (err: unknown) => setSaveError(toCommandError(err).message),
    );
  }

  // Re-registered on each render, so the handler always sees the current page.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (!shouldIgnoreKey(event)) {
        const next = stepPage(page, count, event.key);
        if (next !== page) {
          event.preventDefault();
          goTo(next);
        }
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  // Focus the title when the book opens, so the change is announced and Tab starts here.
  const titleRef = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    titleRef.current?.focus();
  }, []);

  const current = book.pages[page];

  return (
    <>
      <header className="book-header">
        <h1 className="book-title" title={book.title} ref={titleRef} tabIndex={-1}>
          {book.title}
        </h1>
        <button type="button" className="button" onClick={onClose}>
          Close book
        </button>
      </header>
      <main className="book-stage">
        {failedPath !== null && failedPath === current?.path && (
          <div className="status" role="alert">
            <p>This page could not be loaded.</p>
            <button
              type="button"
              className="button"
              onClick={() => {
                setFailedPath(null);
                setAttempt((n) => n + 1);
              }}
            >
              Try again
            </button>
          </div>
        )}
        {current && failedPath !== current.path && (
          <img
            key={`${current.path}#${attempt}`}
            className="page-image"
            src={pageUrl(current.path)}
            width={current.width}
            height={current.height}
            alt={pageAlt(page, count)}
            onError={() => {
              console.error(`page image failed to load: ${pageUrl(current.path)}`);
              setFailedPath(current.path);
            }}
          />
        )}
        {!current && (
          <p className="status">This book has no pages.</p>
        )}
      </main>
      <nav className="book-controls" aria-label="Page navigation">
        <button
          type="button"
          className="button"
          onClick={() => goTo(page - 1)}
          disabled={page <= 0}
        >
          Previous page
        </button>
        <span className="page-indicator" aria-live="polite">
          {pageLabel(page, count)}
        </span>
        {saveError !== null && (
          <output className="save-error">Your place could not be saved. {saveError}</output>
        )}
        <button
          type="button"
          className="button"
          onClick={() => goTo(page + 1)}
          disabled={page >= count - 1}
        >
          Next page
        </button>
      </nav>
    </>
  );
}
