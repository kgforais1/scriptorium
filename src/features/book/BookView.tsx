import { useEffect, useState } from "react";
import { pageUrl, saveProgress } from "../../lib/api";
import type { BookView as Book, CommandError } from "../../lib/bookTypes";
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
  onError: (error: CommandError) => void;
}

/** Whether a key press should leave the page alone: a modifier is held, or a field or dialog has the focus. */
function shouldIgnoreKey(event: KeyboardEvent): boolean {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
    return true;
  }
  return isTypingTarget(event.target) || document.querySelector("dialog[open]") !== null;
}

/** Shows one page of an open book, with the page indicator, Prev/Next buttons, and arrow keys. */
export function BookView({ book, onClose, onError }: BookViewProps) {
  const count = book.pages.length;
  const [page, setPage] = useState(() => clampPage(book.lastReadPage, count));

  function goTo(next: number) {
    if (next === page) {
      return;
    }
    setPage(next);
    saveProgress(book.root, next).catch((err: unknown) => onError(toCommandError(err)));
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

  const current = book.pages[page];

  return (
    <>
      <header className="book-header">
        <h1 className="book-title" title={book.title}>
          {book.title}
        </h1>
        <button type="button" className="button" onClick={onClose}>
          Close book
        </button>
      </header>
      <main className="book-stage">
        {current ? (
          <img
            key={current.path}
            className="page-image"
            src={pageUrl(current.path)}
            width={current.width}
            height={current.height}
            alt={pageAlt(page, count)}
          />
        ) : (
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
