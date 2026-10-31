import { useEffect, useState } from "react";
import daylightRaw from "../../../design/skins/daylight/scene.json";
import lamplitRaw from "../../../design/skins/lamplit/scene.json";
import { lastBook } from "../../lib/api";
import type { BookView } from "../../lib/bookTypes";
import { toCommandError } from "../../lib/errors";
import { parseScene, type Scene } from "./engine";
import { Reader } from "./Reader";
import "./devHarness.css";

const RAW_SCENES: Record<string, unknown> = { lamplit: lamplitRaw, daylight: daylightRaw };
const SCENE_IDS = Object.keys(RAW_SCENES);

/**
 * Dev-only harness at `#spike`. It opens the last book in the real `Reader`, so the chrome and
 * the engine can be checked by eye. A corner switch swaps the scene (lamplit chrome stays) and
 * remounts the reader. Chunk 5 deletes it.
 */
export default function DevHarness() {
  const [book, setBook] = useState<BookView | null>(null);
  const [message, setMessage] = useState<string | null>("Loading…");
  const [mountKey, setMountKey] = useState(0);
  const [sceneId, setSceneId] = useState("lamplit");
  const [scene, setScene] = useState<Scene | undefined>(undefined);

  useEffect(() => {
    let cancelled = false;
    lastBook()
      .then((b) => {
        if (cancelled) return;
        setBook(b);
        setMessage(b ? null : "Open a book in the reader first, then reload this page.");
      })
      .catch((err: unknown) => {
        if (!cancelled) setMessage(toCommandError(err).message);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  function swapScene() {
    const next = SCENE_IDS[(SCENE_IDS.indexOf(sceneId) + 1) % SCENE_IDS.length];
    try {
      setScene(parseScene(RAW_SCENES[next]));
      setSceneId(next);
    } catch (err) {
      setMessage(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div className="dev-harness">
      {book && (
        <Reader
          key={mountKey}
          book={book}
          scene={scene}
          onClose={() => {
            setBook(null);
            setMessage("Book closed. Reload this page to reopen it.");
          }}
        />
      )}
      {message && (
        <output className="dev-harness__message">{message}</output>
      )}
      <div className="dev-harness__tools">
        <button type="button" onClick={swapScene}>
          Scene: {sceneId}
        </button>
        <button type="button" onClick={() => setMountKey((k) => k + 1)}>
          Remount
        </button>
      </div>
    </div>
  );
}
