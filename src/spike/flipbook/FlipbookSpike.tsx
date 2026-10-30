// Spike harness for the flipbook engine. Throwaway: M1 replaces this with the real reader.

import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import lamplitRaw from "../../../design/skins/lamplit/scene.json";
import { FlipbookEngine, type EngineState } from "./engine";
import { parseScene, type Scene } from "./scene";
import daylightRaw from "../../../design/skins/daylight/scene.json";

const RAW_SCENES: Record<string, unknown> = { lamplit: lamplitRaw, daylight: daylightRaw };
const parsed: Record<string, Scene> = {};

// Parsed on first use, so a malformed scene file reaches the error banner instead of breaking the import.
function sceneFor(id: string): Scene {
  if (!(id in RAW_SCENES)) throw new Error(`unknown scene ${id}`);
  return (parsed[id] ??= parseScene(RAW_SCENES[id]));
}

async function listPages(): Promise<string[]> {
  if (isTauri()) {
    const paths = await invoke<string[]>("spike_pages");
    return paths.map((p) => convertFileSrc(p));
  }
  // Browser-only dev mode: Vite serves the repo root, so the gitignored fixture folder is reachable.
  // Resolved against this module's URL, so it is a dev-server path, not a filesystem path.
  const base = `${new URL("../../../tmp/fixtures/spike-pages", import.meta.url).pathname}/`;
  const res = await fetch(base + "manifest.json");
  if (!res.ok) throw new Error("run scripts/make-spike-fixtures.sh first");
  return ((await res.json()) as string[]).map((f) => base + f);
}

const button: React.CSSProperties = {
  minWidth: 44, minHeight: 44, padding: "0 14px", borderRadius: 22, border: "1px solid #f2e8d63d",
  background: "#1a110bcc", color: "#f2e8d6", font: "500 13px system-ui, sans-serif", cursor: "pointer",
};

export default function FlipbookSpike() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const engineRef = useRef<FlipbookEngine | null>(null);
  const [sceneId, setSceneId] = useState(() => new URLSearchParams(location.search).get("scene") ?? "lamplit");
  const [state, setState] = useState<EngineState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mountKey, setMountKey] = useState(0);
  const sceneRef = useRef(sceneId);
  const startFailed = useRef(false);
  sceneRef.current = sceneId;

  // One engine per mount. StrictMode runs this effect twice in development; the first engine is
  // disposed (or its async start aborted) before the second one takes the same canvas.
  useEffect(() => {
    const canvas = canvasRef.current!;
    const ac = new AbortController();
    let engine: FlipbookEngine | null = null;
    setError(null);
    setState(null);
    startFailed.current = false;
    (async () => {
      const pages = await listPages();
      ac.signal.throwIfAborted();
      const startScene = sceneRef.current;
      const onPageError = (page: number) => setError(`Page ${page + 1} failed to load; it shows as blank paper.`);
      engine = await FlipbookEngine.create(canvas, { pages, scene: sceneFor(startScene), onState: setState, onPageError }, ac.signal);
      if (ac.signal.aborted) return engine.dispose();
      // Publish first, so a late scene that fails to parse still leaves working controls.
      engineRef.current = engine;
      // The scene effect skips while startup runs, so apply a scene picked in the meantime.
      if (sceneRef.current !== startScene) engine.setScene(sceneFor(sceneRef.current));
      (window as unknown as { __flipbook?: FlipbookEngine }).__flipbook = engine;
    })().catch((err) => {
      if (ac.signal.aborted) return;
      startFailed.current = true;
      setError(String(err?.message ?? err));
    });
    return () => {
      ac.abort();
      engine?.dispose();
      engineRef.current = null;
    };
  }, [mountKey]);

  useEffect(() => {
    // After a failed start (for example an unknown ?scene=), picking a scene retries the start.
    if (!engineRef.current) {
      if (startFailed.current) setMountKey((k) => k + 1);
      return;
    }
    try {
      engineRef.current.setScene(sceneFor(sceneId));
    } catch (err) {
      setError(String((err as Error)?.message ?? err));
    }
  }, [sceneId]);

  const st = state?.stats;
  return (
    <div style={{ position: "fixed", inset: 0, background: "#0b0705" }}>
      <canvas
        ref={canvasRef}
        aria-label="Book. Drag a page or use the arrow keys to turn."
        style={{ position: "absolute", inset: 0, width: "100%", height: "100%", touchAction: "none", display: "block" }}
      />
      {error && (
        <p role="alert" style={{ position: "absolute", top: 16, left: 16, right: 16, color: "#ffb4a0", font: "14px system-ui" }}>
          {error}
        </p>
      )}
      <div
        style={{
          position: "absolute", left: "50%", bottom: 18, transform: "translateX(-50%)", display: "flex", gap: 8,
          alignItems: "center", padding: 6, borderRadius: 28, background: "#0b0705a6", backdropFilter: "blur(8px)",
          color: "#f2e8d6", font: "13px system-ui",
        }}
      >
        <button style={button} onClick={() => engineRef.current?.prev()} disabled={!state || state.spread === 0} aria-label="Previous spread">
          ‹
        </button>
        <span aria-live="polite" style={{ minWidth: 92, textAlign: "center" }}>
          {state ? `Spread ${state.spread + 1} of ${state.spreads}` : "Loading…"}
        </span>
        <button
          style={button}
          onClick={() => engineRef.current?.next()}
          disabled={!state || state.spread === state.spreads - 1}
          aria-label="Next spread"
        >
          ›
        </button>
        <button style={button} onClick={() => setSceneId((s) => (s === "lamplit" ? "daylight" : "lamplit"))}>
          Scene: {sceneId}
        </button>
        <button style={button} onClick={() => setMountKey((k) => k + 1)} title="Dispose and recreate the engine">
          Remount
        </button>
        {st && (
          <span data-testid="stats" style={{ opacity: 0.6, padding: "0 8px", whiteSpace: "nowrap" }}>
            {st.textures} tex · {st.textureMB.toFixed(0)} MB · upload {st.meanUploadMs.toFixed(0)} ms
          </span>
        )}
      </div>
    </div>
  );
}
