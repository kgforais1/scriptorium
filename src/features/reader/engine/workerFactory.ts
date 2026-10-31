// Creates the decode worker. The `new Worker(new URL(...))` form is what lets Vite find and
// bundle the worker file, so it must stay written exactly like this. Excluded from coverage.

import type { WorkerLike } from "./decoderClient";

/** Starts the decode worker as a module worker. */
export function createBrowserWorker(): WorkerLike {
  return new Worker(new URL("./decodeWorker.ts", import.meta.url), { type: "module" }) as unknown as WorkerLike;
}
