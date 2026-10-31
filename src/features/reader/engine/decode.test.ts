import { afterEach, describe, expect, it, vi } from "vitest";
import { composePage, decodePage, decodeRaw } from "./decode";

afterEach(() => vi.unstubAllGlobals());

const src = { url: "asset://x/p0.jpg", width: 100, height: 140 };

function stubImage(width: number, height: number) {
  const bitmap = { width, height, close: vi.fn() };
  vi.stubGlobal("createImageBitmap", vi.fn(async () => bitmap));
  return bitmap;
}

describe("decodeRaw", () => {
  it("fetches, decodes, and reports the size", async () => {
    const bitmap = stubImage(100, 140);
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, blob: async () => new Blob(["x"]) })));
    const page = await decodeRaw(src, new AbortController().signal);
    expect(page).toEqual({ bitmap, width: 100, height: 140, composed: false });
  });

  it("rejects on a bad status without leaking the path", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, status: 404 })));
    const err = await decodeRaw(src, new AbortController().signal).catch((e: Error) => e);
    expect((err as Error).message).toBe("page fetch failed: HTTP 404");
  });

  it("closes the bitmap and rejects when aborted during decode", async () => {
    const ac = new AbortController();
    const bitmap = { width: 1, height: 1, close: vi.fn() };
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, blob: async () => new Blob(["x"]) })));
    vi.stubGlobal("createImageBitmap", vi.fn(async () => {
      ac.abort();
      return bitmap;
    }));
    await expect(decodeRaw(src, ac.signal)).rejects.toBeTruthy();
    expect(bitmap.close).toHaveBeenCalled();
  });
});

function stubCanvas() {
  const ctx = { fillStyle: "", fillRect: vi.fn(), drawImage: vi.fn(), imageSmoothingQuality: "" };
  const canvas = { width: 0, height: 0, getContext: () => ctx };
  vi.stubGlobal("document", { createElement: () => canvas });
  return { ctx, canvas };
}

describe("decodePage", () => {
  it("is the main-thread decoder: it returns the raw scan and ignores the compose target", async () => {
    const bitmap = stubImage(100, 140);
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, blob: async () => new Blob(["x"]) })));
    const page = await decodePage(src, { boxAspect: 1.4, texH: 512, paper: [1, 1, 1] }, new AbortController().signal);
    expect(page).toEqual({ bitmap, width: 100, height: 140, composed: false });
  });
});

describe("composePage", () => {
  it("makes a 4 px paper square when there is no page", () => {
    const { ctx, canvas } = stubCanvas();
    composePage(null, 1.4, 2048, [1, 0.5, 0]);
    expect([canvas.width, canvas.height]).toEqual([4, 4]);
    expect(ctx.fillStyle).toBe("rgb(255,128,0)");
    expect(ctx.drawImage).not.toHaveBeenCalled();
  });

  it("clamps out-of-range paper channels", () => {
    const { ctx } = stubCanvas();
    composePage(null, 1.4, 4, [2, -1, 0.2]);
    expect(ctx.fillStyle).toBe("rgb(255,0,51)");
  });

  it("letterboxes the scan inside a page box at texture size", () => {
    const { ctx, canvas } = stubCanvas();
    const bitmap = { close: vi.fn() } as unknown as ImageBitmap;
    // A wide scan (200 x 100) in a 1.0-aspect box is centred with bands above and below.
    composePage({ bitmap, width: 200, height: 100, composed: false }, 1, 1000, [1, 1, 1]);
    expect([canvas.width, canvas.height]).toEqual([1000, 1000]);
    expect(ctx.drawImage).toHaveBeenCalledWith(bitmap, 0, 250, 1000, 500);
  });
});
