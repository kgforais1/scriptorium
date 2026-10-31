// The decode worker's logic, free of `self` so it runs in unit tests: fetch the page, decode it,
// letterbox it onto an OffscreenCanvas at the requested size with the requested paper colour,
// and reply with a transferable bitmap that is already composed.

import { paintPage, textureWidth, type Paint2D } from "./compose";
import type { Rgb } from "./scene";

/** Ask the worker to fetch and compose one page. */
export type DecodeRequest = { type: "decode"; id: number; url: string; boxAspect: number; texH: number; paper: Rgb };
/** Cancel a request. The worker aborts its fetch and sends no reply. */
export type AbortRequest = { type: "abort"; id: number };
/** Everything the client sends. */
export type WorkerRequest = DecodeRequest | AbortRequest;

/** Everything the worker sends. `ready` is the one-time probe: `ok` is false when it cannot compose. */
export type WorkerReply =
  | { type: "ready"; ok: boolean }
  | { type: "page"; id: number; bitmap: ImageBitmap; width: number; height: number }
  | { type: "error"; id: number; message: string; stage: ErrorStage };

/**
 * Where a request failed. `fetch` covers the scan itself (a missing file, bad image data), which
 * fails the same way anywhere. `compose` is the worker's own surface, which the main thread may
 * still manage.
 */
export type ErrorStage = "fetch" | "compose";

// A failure of the worker's drawing surface rather than of the scan.
class ComposeFailed extends Error {}

/** A drawing surface of a fixed size. `finish` hands over its pixels as a transferable bitmap. */
export type Surface = { ctx: Paint2D; finish(): ImageBitmap };

/** What the worker needs from its environment. Tests pass fakes. */
export interface WorkerDeps {
  fetch(url: string, init: { signal: AbortSignal }): Promise<{ ok: boolean; status: number; blob(): Promise<Blob> }>;
  createImageBitmap(blob: Blob): Promise<ImageBitmap>;
  /** A surface, or null where OffscreenCanvas with a 2D context is missing. */
  surface(w: number, h: number): Surface | null;
}

/** Sends a reply, transferring the listed objects instead of copying them. */
export type Post = (reply: WorkerReply, transfer: Transferable[]) => void;

async function compose(req: DecodeRequest, deps: WorkerDeps, signal: AbortSignal): Promise<WorkerReply> {
  const res = await deps.fetch(req.url, { signal });
  // The URL holds a local path, so the error carries only the status.
  if (!res.ok) throw new Error(`page fetch failed: HTTP ${res.status}`);
  const blob = await res.blob();
  // A request cancelled while the body downloaded needs no decode.
  signal.throwIfAborted();
  const scan = await deps.createImageBitmap(blob);
  try {
    signal.throwIfAborted();
    const w = textureWidth(req.texH, req.boxAspect);
    let bitmap: ImageBitmap;
    try {
      const surface = deps.surface(w, req.texH);
      if (!surface) throw new Error("OffscreenCanvas is not available in this worker");
      paintPage(surface.ctx, { image: scan, width: scan.width, height: scan.height }, req.boxAspect, w, req.texH, req.paper);
      bitmap = surface.finish();
    } catch (err) {
      throw new ComposeFailed(err instanceof Error ? err.message : String(err));
    }
    return { type: "page", id: req.id, bitmap, width: bitmap.width, height: bitmap.height };
  } finally {
    scan.close();
  }
}

/**
 * Builds the worker's message handler and sends the `ready` probe. Each request runs on its own
 * AbortController, so an abort cancels the fetch and the reply is dropped.
 */
export function createWorkerHandler(deps: WorkerDeps, post: Post): (req: WorkerRequest) => void {
  const running = new Map<number, AbortController>();
  post({ type: "ready", ok: deps.surface(1, 1) !== null }, []);

  // A transfer that fails (a DataCloneError, say) is the worker's problem, not the scan's: free the
  // bitmap and report a compose failure, so the client retries the page on the main thread.
  function postPage(id: number, reply: WorkerReply & { type: "page" }) {
    try {
      post(reply, [reply.bitmap]);
    } catch (err) {
      reply.bitmap.close();
      post({ type: "error", id, message: err instanceof Error ? err.message : String(err), stage: "compose" }, []);
    }
  }

  async function run(req: DecodeRequest) {
    const ac = new AbortController();
    running.set(req.id, ac);
    try {
      const reply = await compose(req, deps, ac.signal);
      if (ac.signal.aborted && reply.type === "page") reply.bitmap.close();
      else if (reply.type === "page") postPage(req.id, reply);
      else post(reply, []);
    } catch (err) {
      if (!ac.signal.aborted) {
        const message = err instanceof Error ? err.message : String(err);
        post({ type: "error", id: req.id, message, stage: err instanceof ComposeFailed ? "compose" : "fetch" }, []);
      }
    } finally {
      running.delete(req.id);
    }
  }

  return (req) => {
    if (req.type === "abort") running.get(req.id)?.abort();
    else void run(req);
  };
}
