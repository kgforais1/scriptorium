import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComposeTarget, DecodedPage, PageSource } from "./decode";
import { browserDecoderEnv, createPageDecoder, workerSupported, type DecoderEnv, type WorkerLike } from "./decoderClient";
import type { WorkerReply, WorkerRequest } from "./workerCore";

const src: PageSource = { url: "asset://x/p0.jpg", width: 100, height: 140 };
const target: ComposeTarget = { boxAspect: 1.4, texH: 512, paper: [1, 1, 1] };
const flush = () => new Promise((r) => setTimeout(r, 0));

class FakeWorker implements WorkerLike {
  sent: WorkerRequest[] = [];
  terminated = false;
  onmessage: ((e: { data: WorkerReply }) => void) | null = null;
  onerror: ((e: unknown) => void) | null = null;
  postMessage(m: WorkerRequest) {
    this.sent.push(m);
  }
  terminate() {
    this.terminated = true;
  }
  reply(r: WorkerReply) {
    this.onmessage?.({ data: r });
  }
}

function env(worker: FakeWorker, over: Partial<DecoderEnv> = {}): DecoderEnv {
  return { supported: true, createWorker: () => worker, probeTimeoutMs: 1000, ...over };
}

// Stubs the main-thread path: fetch plus createImageBitmap.
function stubMainThread() {
  const bitmap = { width: 100, height: 140, close: vi.fn() };
  vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, blob: async () => new Blob(["x"]) })));
  vi.stubGlobal("createImageBitmap", vi.fn(async () => bitmap));
  return bitmap;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("workerSupported", () => {
  const fn = () => {};
  it.each([
    ["all present", { Worker: fn, OffscreenCanvas: fn, createImageBitmap: fn }, true],
    ["no Worker", { OffscreenCanvas: fn, createImageBitmap: fn }, false],
    ["no OffscreenCanvas", { Worker: fn, createImageBitmap: fn }, false],
    ["no createImageBitmap", { Worker: fn, OffscreenCanvas: fn }, false],
    ["nothing", {}, false],
  ])("%s", (_name, g, expected) => {
    expect(workerSupported(g)).toBe(expected);
  });
});

describe("decoder selection", () => {
  it("uses the main thread when the webview lacks the pieces, and never starts a worker", async () => {
    const create = vi.fn();
    const handle = createPageDecoder({ supported: false, createWorker: create, probeTimeoutMs: 10 });
    expect(handle.mode()).toBe("main");
    const bitmap = stubMainThread();
    const page = await handle.decode(src, target, new AbortController().signal);
    expect(page).toEqual({ bitmap, width: 100, height: 140, composed: false });
    expect(create).not.toHaveBeenCalled();
    handle.dispose();
  });

  it("uses the main thread when the worker cannot be created, for example under a CSP", async () => {
    const handle = createPageDecoder({
      supported: true,
      createWorker: () => {
        throw new Error("blocked by CSP");
      },
      probeTimeoutMs: 10,
    });
    expect(handle.mode()).toBe("main");
    stubMainThread();
    expect((await handle.decode(src, target, new AbortController().signal)).composed).toBe(false);
  });

  it("is pending until the worker's probe answers, then uses the worker", () => {
    const w = new FakeWorker();
    const handle = createPageDecoder(env(w));
    expect(handle.mode()).toBe("pending");
    w.reply({ type: "ready", ok: true });
    expect(handle.mode()).toBe("worker");
    handle.dispose();
  });

  it("falls back to the main thread when the worker reports it cannot compose", async () => {
    const w = new FakeWorker();
    const handle = createPageDecoder(env(w));
    stubMainThread();
    const result = handle.decode(src, target, new AbortController().signal);
    w.reply({ type: "ready", ok: false });
    expect((await result).composed).toBe(false);
    expect(handle.mode()).toBe("main");
    expect(w.terminated).toBe(true);
    expect(w.sent).toEqual([]);
  });

  it("falls back when the worker never answers the probe", async () => {
    vi.useFakeTimers();
    const w = new FakeWorker();
    const handle = createPageDecoder(env(w, { probeTimeoutMs: 500 }));
    stubMainThread();
    const result = handle.decode(src, target, new AbortController().signal);
    await vi.advanceTimersByTimeAsync(500);
    expect((await result).composed).toBe(false);
    expect(handle.mode()).toBe("main");
    expect(w.terminated).toBe(true);
  });

  it("falls back when the worker script fails to load", async () => {
    const w = new FakeWorker();
    const handle = createPageDecoder(env(w));
    stubMainThread();
    const result = handle.decode(src, target, new AbortController().signal);
    w.onerror?.(new Event("error"));
    expect((await result).composed).toBe(false);
    expect(handle.mode()).toBe("main");
  });

  it("ignores a late probe after falling back", () => {
    const w = new FakeWorker();
    const handle = createPageDecoder(env(w));
    w.onerror?.(new Event("error"));
    w.reply({ type: "ready", ok: true });
    expect(handle.mode()).toBe("main");
  });
});

describe("worker decoding", () => {
  let w: FakeWorker;
  let handle: ReturnType<typeof createPageDecoder>;
  beforeEach(() => {
    w = new FakeWorker();
    handle = createPageDecoder(env(w));
    w.reply({ type: "ready", ok: true });
  });

  it("sends the page and the compose target, and resolves with a composed page", async () => {
    const result = handle.decode(src, target, new AbortController().signal);
    await flush();
    expect(w.sent).toEqual([{ type: "decode", id: 1, url: src.url, boxAspect: 1.4, texH: 512, paper: [1, 1, 1] }]);
    const bitmap = { close: vi.fn() } as unknown as ImageBitmap;
    w.reply({ type: "page", id: 1, bitmap, width: 366, height: 512 });
    expect(await result).toEqual({ bitmap, width: 366, height: 512, composed: true });
  });

  it("matches replies to requests by id", async () => {
    const a = handle.decode(src, target, new AbortController().signal);
    const b = handle.decode({ ...src, url: "asset://x/p1.jpg" }, target, new AbortController().signal);
    await flush();
    const bmpB = { close: vi.fn() } as unknown as ImageBitmap;
    const bmpA = { close: vi.fn() } as unknown as ImageBitmap;
    w.reply({ type: "page", id: 2, bitmap: bmpB, width: 1, height: 1 });
    w.reply({ type: "page", id: 1, bitmap: bmpA, width: 1, height: 1 });
    expect((await a).bitmap).toBe(bmpA);
    expect((await b).bitmap).toBe(bmpB);
  });

  it("rejects with the worker's message on an error reply", async () => {
    const result = handle.decode(src, target, new AbortController().signal);
    await flush();
    w.reply({ type: "error", id: 1, message: "page fetch failed: HTTP 404", stage: "fetch" });
    await expect(result).rejects.toThrow("page fetch failed: HTTP 404");
  });

  it("retries a page on the main thread when the worker could not compose it, and keeps the worker", async () => {
    stubMainThread();
    const result = handle.decode(src, target, new AbortController().signal);
    await flush();
    w.reply({ type: "error", id: 1, message: "no surface", stage: "compose" });
    expect((await result).composed).toBe(false);
    expect(handle.mode()).toBe("worker");
    expect(w.terminated).toBe(false);
  });

  it("tells the worker to cancel and rejects with the abort reason", async () => {
    const ac = new AbortController();
    const result = handle.decode(src, target, ac.signal);
    await flush();
    ac.abort();
    await expect(result).rejects.toBeInstanceOf(DOMException);
    expect(w.sent[w.sent.length - 1]).toEqual({ type: "abort", id: 1 });
  });

  it("closes a bitmap that arrives after its request was aborted", async () => {
    const ac = new AbortController();
    const result = handle.decode(src, target, ac.signal);
    await flush();
    ac.abort();
    await result.catch(() => {});
    const bitmap = { close: vi.fn() } as unknown as ImageBitmap;
    w.reply({ type: "page", id: 1, bitmap, width: 1, height: 1 });
    expect(bitmap.close).toHaveBeenCalled();
  });

  it("does not post anything for a signal that is already aborted", async () => {
    const ac = new AbortController();
    ac.abort();
    await expect(handle.decode(src, target, ac.signal)).rejects.toBeTruthy();
    expect(w.sent).toEqual([]);
  });

  it("hands pending pages to the main-thread decoder when the worker dies", async () => {
    stubMainThread();
    const result = handle.decode(src, target, new AbortController().signal);
    await flush();
    w.onerror?.(new Event("error"));
    const page: DecodedPage = await result;
    expect(page.composed).toBe(false);
    expect(handle.mode()).toBe("main");
    expect(w.terminated).toBe(true);
  });

  it("decodes later pages on the main thread after the worker died", async () => {
    stubMainThread();
    w.onerror?.(new Event("error"));
    expect((await handle.decode(src, target, new AbortController().signal)).composed).toBe(false);
    expect(w.sent).toEqual([]);
  });

  it("dispose terminates the worker", () => {
    handle.dispose();
    expect(w.terminated).toBe(true);
  });

  it("dispose rejects pending decodes without re-running them on the main thread", async () => {
    const bitmap = stubMainThread();
    const result = handle.decode(src, target, new AbortController().signal);
    const failed = expect(result).rejects.toThrow("disposed");
    await flush();
    handle.dispose();
    await failed;
    expect(globalThis.createImageBitmap).not.toHaveBeenCalled();
    expect(bitmap.close).not.toHaveBeenCalled();
  });

  it("dispose rejects a decode still waiting on the probe", async () => {
    const w2 = new FakeWorker();
    const h2 = createPageDecoder(env(w2));
    stubMainThread();
    const result = h2.decode(src, target, new AbortController().signal);
    h2.dispose();
    await expect(result).rejects.toThrow("disposed");
    expect(globalThis.createImageBitmap).not.toHaveBeenCalled();
  });

  it("rejects at once after dispose, and does not switch to main mode", async () => {
    stubMainThread();
    handle.dispose();
    await expect(handle.decode(src, target, new AbortController().signal)).rejects.toThrow("disposed");
    expect(globalThis.createImageBitmap).not.toHaveBeenCalled();
    expect(handle.mode()).toBe("worker");
    handle.dispose();
  });
});

describe("browserDecoderEnv", () => {
  it("reads support from the given globals' environment and keeps the worker factory", () => {
    const create = vi.fn();
    const e = browserDecoderEnv(create);
    expect(e.createWorker).toBe(create);
    expect(typeof e.supported).toBe("boolean");
    expect(e.probeTimeoutMs).toBeGreaterThan(0);
  });
});
