// Page texture cache. Scans decode off the main path, are letterboxed onto paper (pages.ts),
// and live as GPU textures in a small LRU window around the open spread.

import { textureFrom } from "./gl";
import { composePage, decodePage, type DecodedPage } from "./pages";
import type { Rgb } from "./scene";

export const TEX_CACHE = 12;
export const TEX_HEIGHT = 2048;

export type TextureStats = { textures: number; textureMB: number; lastUploadMs: number; meanUploadMs: number; uploads: number };

type Entry = { tex: WebGLTexture; used: number; bytes: number };

export class PageTextures {
  readonly stats: TextureStats = { textures: 0, textureMB: 0, lastUploadMs: 0, meanUploadMs: 0, uploads: 0 };
  private tex = new Map<number, Entry>();
  private loading = new Set<number>();
  private failed = new Set<number>();
  private abort = new AbortController();
  private paperTex: WebGLTexture;
  private maxTex: number;

  constructor(
    private gl: WebGL2RenderingContext,
    private boxAspect: number,
    private paper: Rgb,
    private urlOf: (slot: number) => string | null,
    private pinned: () => number[],
    private onChange: () => void,
    private onError: (slot: number, err: unknown) => void,
  ) {
    this.paperTex = textureFrom(gl, composePage(null, boxAspect, 4, paper));
    this.maxTex = gl.getParameter(gl.MAX_TEXTURE_SIZE) as number;
  }

  get size() {
    return this.tex.size;
  }

  // Returns the slot's texture, or plain paper while it loads.
  get(slot: number): WebGLTexture {
    const e = this.tex.get(slot);
    if (e) {
      e.used = performance.now();
      return e.tex;
    }
    this.load(slot);
    return this.paperTex;
  }

  load(slot: number) {
    const url = this.urlOf(slot);
    // A failed page is not retried on every frame; it stays plain paper and is reported once.
    if (!url || this.loading.has(slot) || this.tex.has(slot) || this.failed.has(slot)) return;
    this.loading.add(slot);
    decodePage(url, this.abort.signal)
      .then((page) => {
        if (this.abort.signal.aborted) return page.bitmap.close();
        this.upload(slot, page);
        this.onChange();
      })
      .catch((err) => {
        if (this.abort.signal.aborted) return;
        this.failed.add(slot);
        console.error(`[flipbook] page ${slot - 1} failed to load`, err);
        this.onError(slot, err);
      })
      .finally(() => this.loading.delete(slot));
  }

  upload(slot: number, page: DecodedPage) {
    const gl = this.gl, t0 = performance.now();
    // Width is height / boxAspect, so a wide book must lower the height to keep both sides in the GPU limit.
    const max = this.maxTex;
    const texH = Math.max(1, Math.floor(Math.min(TEX_HEIGHT, max, max * this.boxAspect)));
    const canvas = composePage(page, this.boxAspect, texH, this.paper);
    page.bitmap.close();
    // Clear stale error flags, then reject an upload the GPU refused instead of caching it.
    while (gl.getError() !== gl.NO_ERROR);
    const tex = textureFrom(gl, canvas);
    const glErr = gl.getError();
    if (glErr !== gl.NO_ERROR) {
      gl.deleteTexture(tex);
      throw new Error(`texture upload failed: GL error 0x${glErr.toString(16)}`);
    }
    const ms = performance.now() - t0, s = this.stats;
    s.uploads++;
    s.lastUploadMs = ms;
    s.meanUploadMs += (ms - s.meanUploadMs) / s.uploads;
    const old = this.tex.get(slot);
    if (old) gl.deleteTexture(old.tex);
    this.tex.set(slot, { tex, used: performance.now(), bytes: Math.round((canvas.width * canvas.height * 4 * 4) / 3) });
    this.evict(slot);
    s.textures = this.tex.size;
    s.textureMB = [...this.tex.values()].reduce((a, e) => a + e.bytes, 0) / 1048576;
  }

  private evict(keep: number) {
    const pinned = new Set([keep, ...this.pinned()]);
    while (this.tex.size > TEX_CACHE) {
      let old: [number, number] | null = null;
      for (const [k, v] of this.tex) if (!pinned.has(k) && (!old || v.used < old[1])) old = [k, v.used];
      if (!old) return;
      this.gl.deleteTexture(this.tex.get(old[0])!.tex);
      this.tex.delete(old[0]);
    }
  }

  // Drops every texture so pages recompose with a new paper colour. In-flight loads finish
  // with the new colour because composePage runs at upload time.
  setPaper(paper: Rgb) {
    this.paper = paper;
    this.gl.deleteTexture(this.paperTex);
    this.paperTex = textureFrom(this.gl, composePage(null, this.boxAspect, 4, paper));
    for (const e of this.tex.values()) this.gl.deleteTexture(e.tex);
    this.tex.clear();
    this.stats.textures = 0;
    this.stats.textureMB = 0;
  }

  dispose() {
    this.abort.abort();
    for (const e of this.tex.values()) this.gl.deleteTexture(e.tex);
    this.tex.clear();
    this.gl.deleteTexture(this.paperTex);
  }
}
