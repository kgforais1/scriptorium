import { describe, expect, it, vi } from "vitest";
import type { DecodedPage, PageDecoder, PageSource } from "./decode";
import { PageTextures, type TextureBackend } from "./pageTextures";
import type { Rgb } from "./scene";

type Tex = { id: number; paper: Rgb | null; kind: "paper" | "page" };

const PAPER_A: Rgb = [0.9, 0.9, 0.8];
const PAPER_B: Rgb = [0.2, 0.2, 0.3];

// A fake texture backend that records what it made and freed.
function fakeBackend() {
  const made: Tex[] = [];
  const freed: Tex[] = [];
  let failPages = false;
  const backend: TextureBackend<Tex> = {
    paper: (paper) => {
      const t: Tex = { id: made.length, paper, kind: "paper" };
      made.push(t);
      return t;
    },
    page: (_page, paper) => {
      if (failPages) throw new Error("texture upload failed: GL error 0x502");
      const t: Tex = { id: made.length, paper, kind: "page" };
      made.push(t);
      return t;
    },
    free: (t) => freed.push(t),
  };
  return { backend, made, freed, failPages: (v: boolean) => (failPages = v) };
}

const page = (): DecodedPage => ({ bitmap: { close: vi.fn() } as unknown as ImageBitmap, width: 100, height: 140 });
const flush = () => new Promise((r) => setTimeout(r, 0));

function setup(opts: { capacity?: number; decode?: PageDecoder; pinned?: number[]; sources?: number } = {}) {
  const fb = fakeBackend();
  const decode: PageDecoder = opts.decode ?? (async () => page());
  const decodeSpy = vi.fn(decode);
  const onLoad = vi.fn();
  const onError = vi.fn();
  const pinned = opts.pinned ?? [];
  const n = opts.sources ?? 20;
  const host = {
    sourceOf: (slot: number): PageSource | null => (slot >= 1 && slot <= n ? { url: `p${slot - 1}`, width: 100, height: 140 } : null),
    pinned: () => pinned,
    onLoad,
    onError,
  };
  const tex = new PageTextures(fb.backend, decodeSpy, host, PAPER_A, opts.capacity ?? 12);
  return { tex, fb, decodeSpy, onLoad, onError, pinned };
}

describe("PageTextures", () => {
  it("shows paper while a page loads, then the page", async () => {
    const { tex, onLoad } = setup();
    expect(tex.get(3).kind).toBe("paper");
    await flush();
    expect(tex.get(3).kind).toBe("page");
    expect(onLoad).toHaveBeenCalledTimes(1);
  });

  it("does not load a pastedown", async () => {
    const { tex, decodeSpy } = setup({ sources: 3 });
    tex.get(0);
    tex.get(4);
    await flush();
    expect(decodeSpy).not.toHaveBeenCalled();
  });

  it("loads a slot once while it is in flight or cached", async () => {
    const { tex, decodeSpy } = setup();
    tex.load(2);
    tex.load(2);
    await flush();
    tex.load(2);
    expect(decodeSpy).toHaveBeenCalledTimes(1);
  });

  it("evicts down to capacity and frees the evicted texture", () => {
    const { tex, fb } = setup({ capacity: 2 });
    tex.upload(1, page());
    tex.upload(2, page());
    tex.upload(3, page());
    expect(tex.size).toBe(2);
    expect(fb.freed.filter((t) => t.kind === "page")).toHaveLength(1);
  });

  it("keeps pinned slots when evicting", () => {
    const { tex } = setup({ capacity: 2, pinned: [1, 2] });
    tex.upload(1, page());
    tex.upload(2, page());
    tex.upload(3, page());
    expect(tex.has(1)).toBe(true);
    expect(tex.has(2)).toBe(true);
    expect(tex.has(3)).toBe(true);
  });

  it("closes the bitmap after an upload, and after a failed one", () => {
    const { tex, fb } = setup();
    const ok = page();
    tex.upload(1, ok);
    expect(ok.bitmap.close).toHaveBeenCalled();
    fb.failPages(true);
    const bad = page();
    expect(() => tex.upload(2, bad)).toThrow(/GL error/);
    expect(bad.bitmap.close).toHaveBeenCalled();
  });

  describe("setPaper (setScene invalidation)", () => {
    it("rebuilds the paper texture and frees the old one", () => {
      const { tex, fb } = setup();
      const before = tex.get(0);
      expect(before.paper).toEqual(PAPER_A);
      tex.setPaper(PAPER_B);
      const after = tex.get(0);
      expect(after).not.toBe(before);
      expect(after.paper).toEqual(PAPER_B);
      expect(fb.freed).toContain(before);
    });

    it("drops every cached page texture and frees each one", () => {
      const { tex, fb } = setup();
      tex.upload(1, page());
      tex.upload(2, page());
      tex.upload(5, page());
      expect(tex.size).toBe(3);
      tex.setPaper(PAPER_B);
      expect(tex.size).toBe(0);
      expect(fb.freed.filter((t) => t.kind === "page")).toHaveLength(3);
    });

    it("recomposes pages with the new paper colour when they reload", async () => {
      const { tex } = setup();
      tex.upload(1, page());
      tex.setPaper(PAPER_B);
      tex.get(1);
      await flush();
      expect(tex.get(1).paper).toEqual(PAPER_B);
    });

    it("keeps an in-flight load and finishes it with the new colour", async () => {
      const { tex } = setup();
      tex.load(4);
      tex.setPaper(PAPER_B);
      await flush();
      expect(tex.get(4).paper).toEqual(PAPER_B);
    });
  });

  describe("errors", () => {
    it("shows paper and reports once for a page that fails to decode", async () => {
      const { tex, onError, decodeSpy } = setup({ decode: async () => {
          throw new Error("HTTP 404");
        } });
      tex.get(2);
      await flush();
      expect(onError).toHaveBeenCalledTimes(1);
      expect(onError.mock.calls[0][0]).toBe(2);
      expect(tex.get(2).kind).toBe("paper");
      await flush();
      expect(decodeSpy).toHaveBeenCalledTimes(1);
      expect(onError).toHaveBeenCalledTimes(1);
    });

    it("shows paper and reports a GL error on upload instead of caching the page", async () => {
      const { tex, fb, onError, onLoad } = setup();
      fb.failPages(true);
      tex.get(2);
      await flush();
      expect(onError).toHaveBeenCalledTimes(1);
      expect(String(onError.mock.calls[0][1])).toMatch(/GL error/);
      expect(tex.has(2)).toBe(false);
      expect(onLoad).not.toHaveBeenCalled();
    });

    it("accept reports a failed upload through onError", () => {
      const { tex, fb, onError } = setup();
      fb.failPages(true);
      tex.accept(1, page());
      expect(onError).toHaveBeenCalledWith(1, expect.any(Error));
    });

    it("throws from the constructor when the paper texture upload fails", () => {
      const fb = fakeBackend();
      fb.backend.paper = () => {
        throw new Error("texture upload failed: GL error 0x505");
      };
      const host = { sourceOf: () => null, pinned: () => [], onLoad: vi.fn(), onError: vi.fn() };
      expect(() => new PageTextures(fb.backend, async () => page(), host, PAPER_A)).toThrow(/GL error/);
    });
  });

  describe("context loss", () => {
    it("drops textures on suspend, ignores loads, and reloads after resume", async () => {
      const { tex, decodeSpy, onError } = setup();
      tex.upload(1, page());
      tex.suspend();
      expect(tex.size).toBe(0);
      tex.load(2);
      expect(decodeSpy).not.toHaveBeenCalled();
      tex.resume();
      tex.get(1);
      await flush();
      expect(tex.get(1).kind).toBe("page");
      expect(onError).not.toHaveBeenCalled();
    });

    it("discards a decode that finishes while suspended", async () => {
      let finish!: (p: DecodedPage) => void;
      const { tex, onLoad } = setup({ decode: () => new Promise<DecodedPage>((r) => (finish = r)) });
      tex.load(2);
      tex.suspend();
      const late = page();
      finish(late);
      await flush();
      expect(late.bitmap.close).toHaveBeenCalled();
      expect(tex.size).toBe(0);
      expect(onLoad).not.toHaveBeenCalled();
    });

    it("retries a page that failed before the loss", async () => {
      const { tex, decodeSpy } = setup({ decode: async () => {
          throw new Error("x");
        } });
      tex.get(2);
      await flush();
      tex.suspend();
      tex.resume();
      tex.load(2);
      expect(decodeSpy).toHaveBeenCalledTimes(2);
    });
  });

  it("dispose aborts loads, frees textures, and stops reporting", async () => {
    let finish!: (p: DecodedPage) => void;
    const decode: PageDecoder = () => new Promise<DecodedPage>((r) => (finish = r));
    const { tex, fb, onLoad } = setup({ decode });
    tex.upload(1, page());
    tex.load(2);
    tex.dispose();
    const late = page();
    finish(late);
    await flush();
    expect(late.bitmap.close).toHaveBeenCalled();
    expect(onLoad).not.toHaveBeenCalled();
    expect(fb.freed.filter((t) => t.kind === "paper")).toHaveLength(1);
    expect(fb.freed.filter((t) => t.kind === "page")).toHaveLength(1);
  });
});
