// Worker entry: the real environment around `createWorkerHandler`. Loaded by workerFactory.ts.
// Excluded from coverage: it only wires `self` to the tested core.

import { createWorkerHandler, type WorkerReply, type WorkerRequest } from "./workerCore";

type Scope = {
  postMessage(message: unknown, options?: { transfer: Transferable[] }): void;
  onmessage: ((e: { data: WorkerRequest }) => void) | null;
};
const scope = self as unknown as Scope;

const handle = createWorkerHandler(
  {
    fetch: (url, init) => fetch(url, init),
    createImageBitmap: (blob) => createImageBitmap(blob),
    surface: (w, h) => {
      if (typeof OffscreenCanvas === "undefined") return null;
      const canvas = new OffscreenCanvas(w, h);
      const ctx = canvas.getContext("2d");
      return ctx ? { ctx, finish: () => canvas.transferToImageBitmap() } : null;
    },
  },
  (reply: WorkerReply, transfer) => scope.postMessage(reply, { transfer }),
);

scope.onmessage = (e) => handle(e.data);
