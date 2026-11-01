import { useEffect, useState } from "react";
import { ErrorDialog } from "./features/common/ErrorDialog";
import "./features/common/common.css";
import { Home } from "./features/home/Home";
import { Reader } from "./features/reader/Reader";
import { lastBook } from "./lib/api";
import type { BookView as Book, CommandError } from "./lib/bookTypes";
import { toCommandError } from "./lib/errors";

/** What the reader shows: the start-up check, the home screen, or an open book. */
type Screen = { name: "loading" } | { name: "home" } | { name: "book"; book: Book };

/** The shell around the screens: start-up check, home, the reader, and the error dialog. */
function Shell() {
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
        <output className="status">
          Loading…
        </output>
      )}
      {screen.name === "home" && (
        <Home onOpen={(book) => setScreen({ name: "book", book })} onError={setError} />
      )}
      {screen.name === "book" && (
        <Reader key={screen.book.root} book={screen.book} onClose={() => setScreen({ name: "home" })} />
      )}
      {error && <ErrorDialog error={error} onClose={() => setError(null)} />}
    </div>
  );
}

/** Root component: the app shell. */
function App() {
  return <Shell />;
}

export default App;
