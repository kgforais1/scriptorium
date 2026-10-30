import { useEffect, useRef, useState } from "react";
import daylightRaw from "../../../design/skins/daylight/scene.json";
import lamplitRaw from "../../../design/skins/lamplit/scene.json";
import { lastBook, pageUrl } from "../../lib/api";
import type { BookView } from "../../lib/bookTypes";
import { toCommandError } from "../../lib/errors";
import { FlipbookEngine, parseScene, spreadOfPage, type EngineChange, type FlipbookController, type Scene } from "./engine";
import "./devHarness.css";

const RAW_SCENES: Record<string, unknown> = { lamplit: lamplitRaw, daylight: daylightRaw };
const SCENE_IDS = Object.keys(RAW_SCENES);

// Parsed on use, so a malformed scene file reaches the error line instead of breaking the import.
function sceneFor(id: string): Scene {
  return parseScene(RAW_SCENES[id]);
}

function prefersReducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function describe(change: EngineChange | null): string {
  if (!change) return "loading";
  const motion = change.turning ? "turning" : "still";
  return `spread ${change.spread + 1}/${change.spreads} · ${motion} · ${change.loaded} loaded`;
}

type StageProps = { book: BookView; reduced: boolean; engineRef: React.RefObject<FlipbookController | null>; sceneId: React.RefObject<string> };

// One engine per mount. StrictMode runs the effect twice in development; the first engine is
// disposed (or its async start aborted) before the second one takes the same canvas.
function Stage({ book, reduced, engineRef, sceneId }: Readonly<StageProps>) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [change, setChange] = useState<EngineChange | null>(null);
  const [error, setError] = useState<string | null>(null);
  const geometryRef = useRef<HTMLOutputElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current!;
    const ac = new AbortController();
    let engine: FlipbookController | null = null;
    const pages = book.pages.map((p) => ({ url: pageUrl(p.path), width: p.width, height: p.height }));
    FlipbookEngine.create(
      canvas,
      {
        pages,
        scene: sceneFor(sceneId.current),
        startSpread: spreadOfPage(book.lastReadPage),
        reducedMotion: reduced,
        onChange: setChange,
        onError: (e) => setError(e.message),
        // Written straight to the DOM: this fires every frame while a leaf moves.
        onGeometry: (g) => {
          if (geometryRef.current) {
            geometryRef.current.textContent = `pages ${g[0]} | ${g[9]} | ${g[18]}, right-page TL ${g[10].toFixed(0)},${g[11].toFixed(0)}`;
          }
        },
      },
      ac.signal,
    )
      .then((e) => {
        if (ac.signal.aborted) return e.dispose();
        engine = e;
        engineRef.current = e;
      })
      .catch((err: unknown) => {
        if (!ac.signal.aborted) setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      ac.abort();
      engine?.dispose();
      engineRef.current = null;
    };
  }, [book, reduced, engineRef, sceneId]);

  return (
    <>
      <canvas ref={canvasRef} className="dev-harness__canvas" aria-label="Book" />
      {error && (
        <p role="alert" className="dev-harness__error">
          {error}
        </p>
      )}
      <output className="dev-harness__state">
        {describe(change)}
      </output>
      <output ref={geometryRef} className="dev-harness__state dev-harness__geometry" />
    </>
  );
}

/**
 * Dev-only harness for the new flipbook engine, at `#spike`. It mounts the engine on the last
 * opened book with the lamplit scene, and has Prev, Next, Scene swap, Remount, and a reduced
 * motion toggle. Arrow keys turn pages here because the engine has no keyboard listener.
 */
export default function DevHarness() {
  const [book, setBook] = useState<BookView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mountKey, setMountKey] = useState(0);
  const [scene, setScene] = useState("lamplit");
  const [reduced, setReduced] = useState(prefersReducedMotion);
  const engineRef = useRef<FlipbookController | null>(null);
  const sceneId = useRef("lamplit");

  useEffect(() => {
    let cancelled = false;
    lastBook()
      .then((b) => {
        if (cancelled) return;
        if (b) setBook(b);
        else setError("Open a book in the reader first, then reload this page.");
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(toCommandError(err).message);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "ArrowRight" || e.key === "PageDown") engineRef.current?.next();
      else if (e.key === "ArrowLeft" || e.key === "PageUp") engineRef.current?.prev();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const swapScene = () => {
    const next = SCENE_IDS[(SCENE_IDS.indexOf(scene) + 1) % SCENE_IDS.length];
    sceneId.current = next;
    setScene(next);
    try {
      engineRef.current?.setScene(sceneFor(next));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="dev-harness">
      {book ? (
        <Stage key={mountKey} book={book} reduced={reduced} engineRef={engineRef} sceneId={sceneId} />
      ) : (
        <p role={error ? "alert" : "status"} className="dev-harness__error">
          {error ?? "Loading…"}
        </p>
      )}
      <div className="dev-harness__bar">
        <button type="button" onClick={() => engineRef.current?.prev()}>
          Prev
        </button>
        <button type="button" onClick={() => engineRef.current?.next()}>
          Next
        </button>
        <button type="button" onClick={swapScene}>
          Scene: {scene}
        </button>
        <button type="button" onClick={() => setMountKey((k) => k + 1)}>
          Remount
        </button>
        <label>
          <input
            type="checkbox"
            checked={reduced}
            onChange={(e) => setReduced(e.target.checked)}
          />{" "}
          Reduced motion
        </label>
      </div>
    </div>
  );
}
