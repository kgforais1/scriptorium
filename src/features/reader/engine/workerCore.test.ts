import { describe, expect, it, vi } from "vitest";
import type { Paint2D } from "./compose";
import { createWorkerHandler, type DecodeRequest, type WorkerDeps, type WorkerReply } from "./workerCore";

const flush = () => new Promise((r) => setTimeout(r, 0));

const request = (over: Partial<DecodeRequest> = {}): DecodeRequest => ({
  type: "decode", id: 1, url: "asset://x/p0.jpg", boxAspect: 1, texH: 1000, paper: [1, 0.5, 0], ...over,
});

type Posted = { reply: WorkerReply; transfer: Transferable[] };

function setup(over: Partial<WorkerDeps> = {}) {
  const posted: Posted[] = [];
  const ctx = { fillStyle: "", fillRect: vi.fn(), drawImage: vi.fn(), imageSmoothingQuality: "low" } as unknown as Paint2D;
  const scan = { width: 200, height: 100, close: vi.fn() };
  const out = { width: 1000, height: 1000, close: vi.fn() };
  const surface = vi.fn((...size: number[]) => (size.length ? { ctx, finish: () => out as unknown as ImageBitmap } : null));
  const fetchFn = vi.fn(async (...args: [string, { signal: AbortSignal }]) => ({ ok: args.length > 0, status: 200, blob: async () => new Blob(["x"]) }));
  const deps: WorkerDeps = {
    fetch: fetchFn,
    createImageBitmap: async () => scan as unknown as ImageBitmap,
    surface,
    ...over,
  };
  const handle = createWorkerHandler(deps, (reply, transfer) => posted.push({ reply, transfer }));
  return { handle, posted, ctx, scan, out, surface, fetchFn };
}

describe("createWorkerHandler", () => {
  it("answers the probe with ok when a surface can be made", () => {
    const { posted, surface } = setup();
    expect(posted[0].reply).toEqual({ type: "ready", ok: true });
    expect(surface).toHaveBeenCalledWith(1, 1);
  });

  it("answers the probe with not ok where OffscreenCanvas is missing", () => {
    const { posted } = setup({ surface: () => null });
    expect(posted[0].reply).toEqual({ type: "ready", ok: false });
  });

  it("fetches, letterboxes at the requested size and paper colour, and transfers the bitmap", async () => {
    const { handle, posted, ctx, scan, out, surface, fetchFn } = setup();
    handle(request({ boxAspect: 1, texH: 1000, paper: [1, 0.5, 0] }));
    await flush();
    expect(fetchFn.mock.calls[0][0]).toBe("asset://x/p0.jpg");
    expect(surface).toHaveBeenLastCalledWith(1000, 1000);
    expect(ctx.fillStyle).toBe("rgb(255,128,0)");
    expect(ctx.drawImage).toHaveBeenCalledWith(scan, 0, 250, 1000, 500);
    const page = posted[posted.length - 1];
    expect(page.reply).toMatchObject({ type: "page", id: 1, width: 1000, height: 1000 });
    expect(page.transfer).toEqual([out]);
    expect(scan.close).toHaveBeenCalled();
  });

  it("sizes the surface from the box aspect: a landscape box is wider than tall", async () => {
    const { handle, surface } = setup();
    handle(request({ boxAspect: 0.5, texH: 800 }));
    await flush();
    expect(surface).toHaveBeenLastCalledWith(1600, 800);
  });

  it("replies with an error carrying only the status when the fetch fails", async () => {
    const { handle, posted } = setup({ fetch: async () => ({ ok: false, status: 404, blob: async () => new Blob() }) });
    handle(request());
    await flush();
    expect(posted[posted.length - 1].reply).toEqual({ type: "error", id: 1, message: "page fetch failed: HTTP 404", stage: "fetch" });
  });

  it("replies with an error and closes the scan when no surface can be made", async () => {
    let calls = 0;
    const { handle, posted, scan } = setup({ surface: () => (calls++ === 0 ? { ctx: {} as Paint2D, finish: () => ({}) as ImageBitmap } : null) });
    handle(request());
    await flush();
    expect(posted[posted.length - 1].reply).toMatchObject({ type: "error", id: 1, message: expect.stringContaining("OffscreenCanvas"), stage: "compose" });
    expect(scan.close).toHaveBeenCalled();
  });

  it("aborts the fetch and sends no reply when the request is cancelled", async () => {
    let signal!: AbortSignal;
    let release!: () => void;
    const { handle, posted } = setup({
      fetch: (_url, init) => {
        signal = init.signal;
        return new Promise((_resolve, reject) => {
          release = () => reject(new DOMException("aborted", "AbortError"));
        });
      },
    });
    handle(request());
    handle({ type: "abort", id: 1 });
    expect(signal.aborted).toBe(true);
    release();
    await flush();
    expect(posted).toHaveLength(1);
  });

  it("closes a bitmap that finished composing just as the request was cancelled", async () => {
    let finishScan!: () => void;
    const { handle, posted, out } = setup({
      createImageBitmap: () =>
        new Promise<ImageBitmap>((resolve) => {
          finishScan = () => resolve({ width: 200, height: 100, close: vi.fn() } as unknown as ImageBitmap);
        }),
    });
    handle(request());
    await flush();
    handle({ type: "abort", id: 1 });
    finishScan();
    await flush();
    expect(posted).toHaveLength(1);
    expect(out.close).not.toHaveBeenCalled();
  });

  it("ignores an abort for a request that already finished", async () => {
    const { handle, posted } = setup();
    handle(request());
    await flush();
    handle({ type: "abort", id: 1 });
    expect(posted.filter((p) => p.reply.type === "page")).toHaveLength(1);
  });

  it("runs requests independently by id", async () => {
    const { handle, posted } = setup();
    handle(request({ id: 1 }));
    handle(request({ id: 2 }));
    await flush();
    const ids = posted.flatMap((p) => (p.reply.type === "page" ? [p.reply.id] : []));
    expect(ids).toHaveLength(2);
    expect(ids).toContain(1);
    expect(ids).toContain(2);
  });
});
