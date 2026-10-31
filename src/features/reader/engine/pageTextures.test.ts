import { describe, expect, it, vi } from "vitest";
import type { ComposeTarget, DecodedPage, PageDecoder, PageSource } from "./decode";
import { PageTextures, type TextureBackend } from "./pageTextures";
import type { Rgb } from "./scene";

type Tex = { id: number; paper: Rgb | null; kind: "paper" | "page"; texH: number | null; composed: boolean };
// A composed page remembers the paper colour the "worker" used, so tests can see a stale one.
type Stamped = { paper?: Rgb; close(): void };

const BOX = 1.4;

const PAPER_A: Rgb = [0.9, 0.9, 0.8];
const PAPER_B: Rgb = [0.2, 0.2, 0.3];

// A fake texture backend that records what it made and freed.
function fakeBackend() {
  const made: Tex[] = [];
  const freed: Tex[] = [];
  let failPages = false;
  const backend: TextureBackend<Tex> = {
    paper: (paper) => {
      const t: Tex = { id: made.length, paper, kind: "paper", texH: null, composed: false };
      made.push(t);
      return t;
    },
    page: (p, paper, texH) => {
      if (failPages) throw new Error("texture upload failed: GL error 0x502");
      const stamp = (p.bitmap as unknown as Stamped).paper;
      const t: Tex = { id: made.length, paper: p.composed ? (stamp ?? null) : paper, kind: "page", texH: p.composed ? p.height : texH, composed: p.composed };
      made.push(t);
      return t;
    },
    free: (t) => freed.push(t),
  };
  return { backend, made, freed, failPages: (v: boolean) => (failPages = v) };
}

// A raw scan, which the backend letterboxes.
const page = (): DecodedPage => ({ bitmap: { close: vi.fn() } as unknown as ImageBitmap, width: 100, height: 140, composed: false });
// A page the worker already composed to the target.
const composed = (t: ComposeTarget): DecodedPage => ({
  bitmap: { close: vi.fn(), paper: t.paper } as unknown as ImageBitmap,
  width: Math.round(t.texH / t.boxAspect),
  height: t.texH,
  composed: true,
});
const flush = () => new Promise((r) => setTimeout(r, 0));

function setup(opts: { capacity?: number; decode?: PageDecoder; pinned?: number[]; sources?: number } = {}) {
  const fb = fakeBackend();
  const size = { texH: 1024 };
  const decode: PageDecoder = opts.decode ?? (async (_source, target) => composed(target));
  const decodeSpy = vi.fn(decode);
  const onLoad = vi.fn();
  const onError = vi.fn();
  const pinned = opts.pinned ?? [];
  const n = opts.sources ?? 20;
  const host = {
    boxAspect: BOX,
    texHeight: () => size.texH,
    sourceOf: (slot: number): PageSource | null => (slot >= 1 && slot <= n ? { url: `p${slot - 1}`, width: 100, height: 140 } : null),
    pinned: () => pinned,
    onLoad,
    onError,
  };
  const tex = new PageTextures(fb.backend, decodeSpy, host, PAPER_A, opts.capacity ?? 12);
  return { tex, fb, decodeSpy, onLoad, onError, pinned, size };
}

// Loads slots at a tiny height, as when the canvas was laid out at 1 px.
async function loadTiny(tex: ReturnType<typeof setup>, slots: number[], texH = 2) {
  tex.size.texH = texH;
  for (const s of slots) tex.tex.load(s);
  await flush();
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

    it("changes nothing when the new paper texture cannot be built", () => {
      const { tex, fb } = setup();
      tex.upload(1, page());
      const paperBefore = tex.get(0);
      const freedBefore = fb.freed.length;
      const real = fb.backend.paper;
      fb.backend.paper = () => {
        throw new Error("texture upload failed: GL error 0x505");
      };
      expect(() => tex.setPaper(PAPER_B)).toThrow(/GL error/);
      expect(tex.size).toBe(1);
      expect(tex.get(0)).toBe(paperBefore);
      expect(fb.freed).toHaveLength(freedBefore);
      // The old colour is still what new pages are composed with.
      fb.backend.paper = real;
      tex.upload(2, page());
      expect(tex.get(2).paper).toEqual(PAPER_A);
    });

    it("only records the colour and clears the cache while suspended", () => {
      const { tex, fb } = setup();
      tex.upload(1, page());
      tex.suspend();
      const made = fb.made.length;
      fb.backend.paper = () => {
        throw new Error("no GPU while suspended");
      };
      expect(() => tex.setPaper(PAPER_B)).not.toThrow();
      expect(tex.size).toBe(0);
      expect(fb.made).toHaveLength(made);
    });

    it("rebuilds the paper from the colour set while suspended when it resumes", () => {
      const { tex } = setup();
      tex.suspend();
      tex.setPaper(PAPER_B);
      tex.resume();
      expect(tex.get(0).paper).toEqual(PAPER_B);
    });

    it("recomposes pages with the new paper colour when they reload", async () => {
      const { tex } = setup();
      tex.upload(1, page());
      tex.setPaper(PAPER_B);
      tex.get(1);
      await flush();
      expect(tex.get(1).paper).toEqual(PAPER_B);
    });

    it("aborts an in-flight load and drops its page, which carries the old colour", async () => {
      const resolvers: ((p: DecodedPage) => void)[] = [];
      const signals: AbortSignal[] = [];
      const decode: PageDecoder = (...args) =>
        new Promise<DecodedPage>((resolve) => {
          signals.push(args[2]);
          resolvers.push((p) => resolve(p));
        });
      const { tex, onLoad } = setup({ decode });
      tex.load(4);
      tex.setPaper(PAPER_B);
      expect(signals[0].aborted).toBe(true);
      const stale = composed({ boxAspect: BOX, texH: 1024, paper: PAPER_A });
      resolvers[0](stale);
      await flush();
      expect(stale.bitmap.close).toHaveBeenCalled();
      expect(tex.has(4)).toBe(false);
      expect(onLoad).not.toHaveBeenCalled();
    });

    it("reloads a dropped page with the new colour on demand", async () => {
      const { tex, decodeSpy } = setup();
      tex.load(4);
      tex.setPaper(PAPER_B);
      await flush();
      expect(tex.has(4)).toBe(false);
      tex.get(4);
      await flush();
      expect(decodeSpy).toHaveBeenCalledTimes(2);
      expect(decodeSpy.mock.calls[1][1].paper).toEqual(PAPER_B);
      expect(tex.get(4).paper).toEqual(PAPER_B);
    });

    it("does not start a third decode while the restarted one is in flight", async () => {
      const resolvers: (() => void)[] = [];
      const decode: PageDecoder = (_s, target) => new Promise<DecodedPage>((resolve) => resolvers.push(() => resolve(composed(target))));
      const { tex, decodeSpy } = setup({ decode });
      tex.load(4);
      tex.setPaper(PAPER_B);
      tex.load(4);
      resolvers[0]();
      await flush();
      tex.load(4);
      expect(decodeSpy).toHaveBeenCalledTimes(2);
      resolvers[1]();
      await flush();
      expect(tex.has(4)).toBe(true);
    });
  });

  describe("texture size and composed pages", () => {
    it("hands the decoder the page box, the current texture height, and the paper colour", () => {
      const { tex, decodeSpy, size } = setup();
      size.texH = 777;
      tex.load(2);
      expect(decodeSpy.mock.calls[0][1]).toEqual({ boxAspect: BOX, texH: 777, paper: PAPER_A });
    });

    it("keeps uploaded textures at their size and uses the new size for new uploads", async () => {
      const { tex, size } = setup();
      size.texH = 2048;
      tex.load(1);
      await flush();
      size.texH = 512;
      tex.load(2);
      await flush();
      expect(tex.get(1).texH).toBe(2048);
      expect(tex.get(2).texH).toBe(512);
    });

    it("uploads a composed page as it came and passes the current height for a raw scan", () => {
      const { tex, size } = setup();
      size.texH = 640;
      tex.upload(1, composed({ boxAspect: BOX, texH: 300, paper: PAPER_A }));
      tex.upload(2, page());
      expect(tex.get(1)).toMatchObject({ composed: true, texH: 300 });
      expect(tex.get(2)).toMatchObject({ composed: false, texH: 640, paper: PAPER_A });
    });
  });

  describe("refresh (a bigger target replaces small textures)", () => {
    it("rebuilds textures built much smaller than the target, pinned ones included", async () => {
      const t = setup({ pinned: [1, 2] });
      await loadTiny(t, [1, 2, 3]);
      expect(t.tex.get(1).texH).toBe(2);
      t.size.texH = 800;
      t.tex.refresh();
      await flush();
      for (const slot of [1, 2, 3]) expect(t.tex.get(slot).texH).toBe(800);
      expect(t.tex.size).toBe(3);
    });

    it("keeps the old texture on screen until the new one lands, then frees it", async () => {
      const t = setup();
      await loadTiny(t, [1]);
      const old = t.tex.get(1);
      t.size.texH = 800;
      t.tex.refresh();
      expect(t.tex.get(1)).toBe(old);
      expect(t.fb.freed).not.toContain(old);
      await flush();
      expect(t.tex.get(1)).not.toBe(old);
      expect(t.fb.freed).toContain(old);
      expect(t.onLoad).toHaveBeenCalledTimes(2);
    });

    it("asks the decoder for the new height and the current paper", async () => {
      const t = setup();
      await loadTiny(t, [1]);
      t.size.texH = 800;
      t.tex.refresh();
      expect(t.decodeSpy.mock.calls[1][1]).toEqual({ boxAspect: BOX, texH: 800, paper: PAPER_A });
    });

    it("leaves textures alone for a small resize or a smaller target", async () => {
      const t = setup();
      await loadTiny(t, [1, 2], 800);
      t.size.texH = 900;
      t.tex.refresh();
      t.size.texH = 300;
      t.tex.refresh();
      await flush();
      expect(t.decodeSpy).toHaveBeenCalledTimes(2);
    });

    it("does not rebuild a texture twice while the rebuild is in flight", async () => {
      const t = setup();
      await loadTiny(t, [1]);
      t.size.texH = 800;
      t.tex.refresh();
      t.tex.refresh();
      await flush();
      expect(t.decodeSpy).toHaveBeenCalledTimes(2);
    });

    it("rebuilds raw (main-thread fallback) textures at the new height too", async () => {
      const t = setup({ decode: async () => page() });
      await loadTiny(t, [1], 100);
      expect(t.tex.get(1)).toMatchObject({ composed: false, texH: 100 });
      t.size.texH = 800;
      t.tex.refresh();
      await flush();
      expect(t.tex.get(1)).toMatchObject({ composed: false, texH: 800 });
    });

    it("keeps the old texture and reports nothing when a rebuild fails to decode", async () => {
      let fail = false;
      const t = setup({
        decode: async (_s, target) => {
          if (fail) throw new Error("HTTP 500");
          return composed(target);
        },
      });
      await loadTiny(t, [1]);
      const old = t.tex.get(1);
      fail = true;
      t.size.texH = 800;
      t.tex.refresh();
      await flush();
      expect(t.tex.get(1)).toBe(old);
      expect(t.onError).not.toHaveBeenCalled();
      // The next refresh tries again.
      fail = false;
      t.tex.refresh();
      await flush();
      expect(t.tex.get(1).texH).toBe(800);
    });

    it("keeps the old texture and reports nothing when the GPU refuses the rebuild", async () => {
      const t = setup();
      await loadTiny(t, [1]);
      const old = t.tex.get(1);
      t.fb.failPages(true);
      t.size.texH = 800;
      t.tex.refresh();
      await flush();
      expect(t.tex.get(1)).toBe(old);
      expect(t.onError).not.toHaveBeenCalled();
      expect(t.fb.freed).not.toContain(old);
    });

    it("drops a rebuild whose slot was evicted while it decoded", async () => {
      let finish!: () => void;
      let slow = false;
      const t = setup({
        capacity: 1,
        decode: (_s, target) =>
          slow ? new Promise<DecodedPage>((resolve) => (finish = () => resolve(composed(target)))) : Promise.resolve(composed(target)),
      });
      await loadTiny(t, [1]);
      slow = true;
      t.size.texH = 800;
      t.tex.refresh();
      slow = false;
      t.tex.upload(2, composed({ boxAspect: BOX, texH: 800, paper: PAPER_A }));
      expect(t.tex.has(1)).toBe(false);
      finish();
      await flush();
      expect(t.tex.has(1)).toBe(false);
      expect(t.tex.size).toBe(1);
    });

    it("aborts a decode in flight that would land too small and lets it restart on demand", async () => {
      const signals: AbortSignal[] = [];
      const t = setup({
        decode: (...args) => {
          signals.push(args[2]);
          return new Promise<DecodedPage>(() => undefined);
        },
      });
      t.size.texH = 2;
      t.tex.load(1);
      t.size.texH = 800;
      t.tex.refresh();
      expect(signals[0].aborted).toBe(true);
      t.tex.load(1);
      expect(signals).toHaveLength(2);
      expect(t.decodeSpy.mock.calls[1][1].texH).toBe(800);
    });

    it("lets an in-flight decode at a fitting size finish", async () => {
      const signals: AbortSignal[] = [];
      const t = setup({
        decode: (_s, target, signal) => {
          signals.push(signal);
          return new Promise<DecodedPage>((resolve) => setTimeout(() => resolve(composed(target)), 0));
        },
      });
      t.size.texH = 780;
      t.tex.load(1);
      t.size.texH = 800;
      t.tex.refresh();
      expect(signals[0].aborted).toBe(false);
      await flush();
      expect(t.tex.has(1)).toBe(true);
    });

    it("does nothing while suspended", async () => {
      const t = setup();
      await loadTiny(t, [1]);
      t.tex.suspend();
      t.size.texH = 800;
      t.tex.refresh();
      expect(t.decodeSpy).toHaveBeenCalledTimes(1);
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
      const host = { boxAspect: BOX, texHeight: () => 512, sourceOf: () => null, pinned: () => [], onLoad: vi.fn(), onError: vi.fn() };
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

    it("aborts the decodes in flight and closes what they return", async () => {
      let signal!: AbortSignal;
      let finish!: (p: DecodedPage) => void;
      const decode: PageDecoder = (_s, _t, sig) =>
        new Promise<DecodedPage>((resolve) => {
          signal = sig;
          finish = resolve;
        });
      const { tex, onLoad, onError } = setup({ decode });
      tex.load(2);
      tex.suspend();
      expect(signal.aborted).toBe(true);
      const late = composed({ boxAspect: BOX, texH: 512, paper: PAPER_A });
      finish(late);
      await flush();
      expect(late.bitmap.close).toHaveBeenCalled();
      expect(onLoad).not.toHaveBeenCalled();
      expect(onError).not.toHaveBeenCalled();
      tex.resume();
      tex.load(2);
      expect(tex.has(2)).toBe(false);
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

describe("PageTextures.whenSettled", () => {
  it("resolves after the slots have loaded", async () => {
    const { tex } = setup();
    await tex.whenSettled([2, 3]);
    expect(tex.has(2)).toBe(true);
    expect(tex.has(3)).toBe(true);
  });

  it("resolves at once for cached slots and pastedowns", async () => {
    const { tex, decodeSpy } = setup({ sources: 3 });
    tex.upload(1, page());
    await tex.whenSettled([0, 1, 4]);
    expect(decodeSpy).not.toHaveBeenCalled();
  });

  it("waits for a slow decode", async () => {
    let finish!: (p: DecodedPage) => void;
    const decode: PageDecoder = () => new Promise<DecodedPage>((r) => (finish = r));
    const { tex } = setup({ decode });
    let settled = false;
    const wait = tex.whenSettled([2]).then(() => (settled = true));
    await flush();
    expect(settled).toBe(false);
    finish(page());
    await wait;
    expect(tex.has(2)).toBe(true);
  });

  it("counts a failed page as settled, after reporting it", async () => {
    const decode: PageDecoder = async () => {
      throw new Error("bad scan");
    };
    const { tex, onError } = setup({ decode });
    await tex.whenSettled([2, 3]);
    expect(onError).toHaveBeenCalledTimes(2);
    expect(tex.has(2)).toBe(false);
  });

  it("does not hang when the loads are aborted", async () => {
    const decode: PageDecoder = (_s, _t, signal) =>
      new Promise<DecodedPage>((_, reject) => signal.addEventListener("abort", () => reject(signal.reason)));
    const { tex } = setup({ decode });
    const wait = tex.whenSettled([2]);
    tex.dispose();
    await wait;
  });
});
