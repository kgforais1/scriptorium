import { useState } from "react";
import { inspectFolder, openBook, pickFolder } from "../../lib/api";
import type { BookView, CommandError, ScansFolderInfo } from "../../lib/bookTypes";
import { toCommandError } from "../../lib/errors";
import { ConfirmIngest } from "./ConfirmIngest";
import "./home.css";

interface HomeProps {
  /** Called with a book to show. */
  onOpen: (book: BookView) => void;
  onError: (error: CommandError) => void;
}

interface PendingScans {
  folder: string;
  info: ScansFolderInfo;
}

/** The start screen: one Open button that opens a book or imports a folder of scans. */
export function Home({ onOpen, onError }: Readonly<HomeProps>) {
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<PendingScans | null>(null);

  async function choose() {
    setBusy(true);
    try {
      const folder = await pickFolder();
      if (folder === null) {
        return;
      }
      const info = await inspectFolder(folder);
      if (info.kind === "book") {
        onOpen(await openBook(folder));
      } else {
        setPending({ folder, info });
      }
    } catch (err) {
      onError(toCommandError(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="home">
      <h1>Scriptorium</h1>
      <p>Open a book, or a folder of scanned pages.</p>
      <button type="button" className="button primary" onClick={() => void choose()} disabled={busy}>
        Open…
      </button>
      {pending && (
        <ConfirmIngest
          folder={pending.folder}
          info={pending.info}
          onCreated={onOpen}
          onCancel={() => setPending(null)}
          onError={onError}
        />
      )}
    </main>
  );
}
