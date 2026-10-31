// Main-thread side of the decode worker, behind the `PageDecoder` interface. It picks the worker
// path where the webview can compose off the main thread, and the main-thread decoder otherwise.
// The pick is made once, from a probe the worker answers, and a worker that fails later hands its
// pending pages to the main-thread decoder, so an older webview is slower, not broken.

import { decodePage, type ComposeTarget, type DecodedPage, type PageDecoder, type PageSource } from "./decode";
import type { WorkerReply, WorkerRequest } from "./workerCore";

/** The slice of `Worker` the client uses. Tests pass a fake. */
export interface WorkerLike {
  postMessage(message: WorkerRequest): void;
  terminate(): void;
  onmessage: ((e: { data: WorkerReply }) => void) | null;
  onerror: ((e: unknown) => void) | null;
}

/** What the client needs from the environment. */
export interface DecoderEnv {
  /** Whether the webview has Worker, OffscreenCanvas, and createImageBitmap at all. */
  supported: boolean;
  /** Starts the worker. May throw, for example when a CSP blocks it. */
  createWorker(): WorkerLike;
  /** How long to wait for the worker's probe before falling back, in milliseconds. */
  probeTimeoutMs: number;
}

/** A decoder plus the way to stop it. */
export interface PageDecoderHandle {
  /** Decodes through the worker, or on the main thread when the worker is not usable. */
  readonly decode: PageDecoder;
  /** Which path is in use. `pending` until the worker's probe has answered. */
  mode(): "pending" | "worker" | "main";
  /** Terminates the worker. Pending requests reject, and later decodes reject at once. */
  dispose(): void;
}

/** Whether the main thread has the pieces the worker path needs. */
export function workerSupported(g: { Worker?: unknown; OffscreenCanvas?: unknown; createImageBitmap?: unknown }): boolean {
  return typeof g.Worker === "function" && typeof g.OffscreenCanvas === "function" && typeof g.createImageBitmap === "function";
}

type Pending = { resolve(page: DecodedPage): void; reject(err: unknown): void };

// Raised for every request still waiting when the worker dies, so the caller can retry on the main thread.
class WorkerFailed extends Error {}
const DISPOSED = "the page decoder was disposed";

/** Builds the decoder for an environment. Never throws: a worker that cannot start means the main-thread path. */
export function createPageDecoder(env: DecoderEnv): PageDecoderHandle {
  if (!env.supported) return { decode: decodePage, mode: () => "main", dispose() {} };
  let worker: WorkerLike;
  try {
    worker = env.createWorker();
  } catch {
    return { decode: decodePage, mode: () => "main", dispose() {} };
  }

  let mode: "pending" | "worker" | "main" = "pending";
  let disposed = false;
  let nextId = 0;
  const pending = new Map<number, Pending>();
  let settleProbe!: (ok: boolean) => void;
  const probe = new Promise<boolean>((resolve) => (settleProbe = resolve));
  const timer = setTimeout(() => fallBack(), env.probeTimeoutMs);

  function fallBack(): void {
    clearTimeout(timer);
    if (mode !== "main") {
      mode = "main";
      worker.terminate();
    }
    settleProbe(false);
    for (const p of pending.values()) p.reject(new WorkerFailed("the page decoder worker stopped"));
    pending.clear();
  }

  worker.onerror = () => fallBack();
  worker.onmessage = ({ data }) => {
    if (data.type === "ready") {
      clearTimeout(timer);
      if (data.ok && mode === "pending") mode = "worker";
      else if (!data.ok) fallBack();
      settleProbe(mode === "worker");
      return;
    }
    const p = pending.get(data.id);
    pending.delete(data.id);
    if (!p) {
      // Aborted while the reply was in flight: nobody will upload it.
      if (data.type === "page") data.bitmap.close();
    } else if (data.type === "page") p.resolve({ bitmap: data.bitmap, width: data.width, height: data.height, composed: true });
    // A scan that failed to compose in the worker gets one more try on the main thread.
    else p.reject(data.stage === "compose" ? new WorkerFailed(data.message) : new Error(data.message));
  };

  function viaWorker(page: PageSource, target: ComposeTarget, signal: AbortSignal): Promise<DecodedPage> {
    signal.throwIfAborted();
    return new Promise<DecodedPage>((resolve, reject) => {
      const id = ++nextId;
      const onAbort = () => {
        pending.delete(id);
        worker.postMessage({ type: "abort", id });
        reject(signal.reason);
      };
      signal.addEventListener("abort", onAbort, { once: true });
      pending.set(id, {
        resolve: (p) => {
          signal.removeEventListener("abort", onAbort);
          resolve(p);
        },
        reject: (e) => {
          signal.removeEventListener("abort", onAbort);
          reject(e);
        },
      });
      worker.postMessage({ type: "decode", id, url: page.url, ...target });
    });
  }

  const decode: PageDecoder = async (page, target, signal) => {
    if (disposed) throw new Error(DISPOSED);
    const usable = await probe;
    if (disposed) throw new Error(DISPOSED);
    if (!usable || mode !== "worker") return decodePage(page, target, signal);
    try {
      return await viaWorker(page, target, signal);
    } catch (err) {
      if (err instanceof WorkerFailed) return decodePage(page, target, signal);
      throw err;
    }
  };

  return {
    decode,
    mode: () => mode,
    dispose() {
      if (disposed) return;
      disposed = true;
      clearTimeout(timer);
      worker.terminate();
      settleProbe(false);
      // A plain Error, not WorkerFailed: decode() must not retry these on the main thread.
      for (const p of pending.values()) p.reject(new Error(DISPOSED));
      pending.clear();
    },
  };
}

/** The real environment: the browser's globals and the bundled worker. */
export function browserDecoderEnv(createWorker: () => WorkerLike): DecoderEnv {
  return { supported: workerSupported(globalThis), createWorker, probeTimeoutMs: 5000 };
}
