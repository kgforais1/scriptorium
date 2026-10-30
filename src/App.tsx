import { lazy, Suspense, useEffect, useState, useSyncExternalStore } from "react";
import { BookView } from "./features/book/BookView";
import { ErrorDialog } from "./features/common/ErrorDialog";
import "./features/common/common.css";
import { Home } from "./features/home/Home";
import { lastBook } from "./lib/api";
import type { BookView as Book, CommandError } from "./lib/bookTypes";
import { toCommandError } from "./lib/errors";

// The flipbook spike exists in dev builds only. The constant condition lets the bundler drop
// the dynamic import, so release builds never include the spike or depend on its page command.
const Spike = import.meta.env.DEV ? lazy(() => import("./spike/flipbook/FlipbookSpike")) : null;

function subscribeToHash(notify: () => void): () => void {
  window.addEventListener("hashchange", notify);
  return () => window.removeEventListener("hashchange", notify);
}

function useIsSpikeRoute(): boolean {
  const hash = useSyncExternalStore(subscribeToHash, () => window.location.hash);
  return Spike !== null && hash === "#spike";
}

/** What the reader shows: the start-up check, the home screen, or an open book. */
type Screen = { name: "loading" } | { name: "home" } | { name: "book"; book: Book };

function Reader() {
  const [screen, setScreen] = useState<Screen>({ name: "loading" });
  const [error, setError] = useState<CommandError | null>(null);

  useEffect(() => {
    let cancelled = false;
    lastBook()
      .then((book) => {
        if (!cancelled) {
          setScreen(book ? { name: "book", book } : { name: "home" });
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setScreen({ name: "home" });
          setError(toCommandError(err));
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="app">
      {screen.name === "loading" && (
        <p className="status" role="status">
          Loading…
        </p>
      )}
      {screen.name === "home" && (
        <Home onOpen={(book) => setScreen({ name: "book", book })} onError={setError} />
      )}
      {screen.name === "book" && (
        <BookView
          key={screen.book.root}
          book={screen.book}
          onClose={() => setScreen({ name: "home" })}
          onError={setError}
        />
      )}
      {error && <ErrorDialog error={error} onClose={() => setError(null)} />}
    </div>
  );
}

/** Root component: the reader, or in dev builds the flipbook spike at `#spike`. */
function App() {
  const showSpike = useIsSpikeRoute();
  if (showSpike && Spike) {
    return (
      <Suspense fallback={null}>
        <Spike />
      </Suspense>
    );
  }
  return <Reader />;
}

export default App;
