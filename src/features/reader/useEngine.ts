import { useEffect, useRef, useState } from "react";
import { pageUrl } from "../../lib/api";
import type { BookView } from "../../lib/bookTypes";
import { FlipbookEngine, spreadOfPage, type EngineChange, type EngineError, type FlipbookController, type Scene } from "./engine";

/** Where the engine is: starting (first spread decoding), running, or failed to start. */
export type EngineStatus = "loading" | "ready" | "error";

type Options = {
  book: BookView;
  scene: Scene;
  reduced: boolean;
  /** Called on every discrete engine change. */
  onChange: (change: EngineChange) => void;
  /** Called when one page fails to load. The book stays open. */
  onPageError: (error: EngineError) => void;
};

/**
 * Runs one flipbook engine on a canvas for as long as the book is open. It starts again when the
 * book changes (give `Reader` a `key` per book so its state resets), follows scene and reduced-motion changes, and frees the engine on unmount. A book
 * with no pages starts no engine.
 *
 * @returns The canvas ref to attach, the controller ref, the status, and the start error message.
 */
export function useEngine({ book, scene, reduced, onChange, onPageError }: Readonly<Options>) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const controllerRef = useRef<FlipbookController | null>(null);
  // Keyed by book, so a new book reads as loading until its own engine starts.
  const [started, setStarted] = useState<{ book: BookView; status: EngineStatus; error: string | null } | null>(null);
  const status: EngineStatus = started?.book === book ? started.status : "loading";
  const error = started?.book === book ? started.error : null;
  // Read when the engine starts. Later changes go through the two effects below.
  const latest = useRef({ scene, reduced, onChange, onPageError });
  const appliedScene = useRef(scene);

  useEffect(() => {
    latest.current = { scene, reduced, onChange, onPageError };
  });

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || book.pages.length === 0) return;
    const ac = new AbortController();
    appliedScene.current = latest.current.scene;
    FlipbookEngine.create(
      canvas,
      {
        pages: book.pages.map((p) => ({ url: pageUrl(p.path), width: p.width, height: p.height })),
        scene: latest.current.scene,
        startSpread: spreadOfPage(book.lastReadPage),
        reducedMotion: latest.current.reduced,
        onChange: (c) => latest.current.onChange(c),
        onError: (e) => latest.current.onPageError(e),
      },
      ac.signal,
    ).then(
      (engine) => {
        if (ac.signal.aborted) return engine.dispose();
        controllerRef.current = engine;
        setStarted({ book, status: "ready", error: null });
      },
      (err: unknown) => {
        if (ac.signal.aborted) return;
        setStarted({ book, status: "error", error: err instanceof Error ? err.message : String(err) });
      },
    );
    return () => {
      ac.abort();
      controllerRef.current?.dispose();
      controllerRef.current = null;
    };
  }, [book]);

  // Also runs when the engine becomes ready, so a scene picked while it loaded is not lost.
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || appliedScene.current === scene) return;
    appliedScene.current = scene;
    controller.setScene(scene);
  }, [scene, status]);

  useEffect(() => {
    controllerRef.current?.setReducedMotion(reduced);
  }, [reduced, status]);

  return { canvasRef, controllerRef, status, error };
}
