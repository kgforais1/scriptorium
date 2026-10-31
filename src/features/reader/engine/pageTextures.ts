// Page texture cache. Pages decode through a `PageDecoder`, which letterboxes them onto paper
// (the worker does it off the main thread; the backend does it for a raw scan), and live as GPU
// textures in a small LRU window around the open spread. The cache is generic over the texture
// type and knows nothing about WebGL, so tests use a fake backend.

import type { ComposeTarget, DecodedPage, PageDecoder, PageSource } from "./decode";
import { LruCache } from "./lruCache";
import { needsReload, slotsToReload } from "./texReload";
import type { Rgb } from "./scene";

/** Textures kept around the open spread. */
export const TEX_CACHE = 12;

/** Creates and frees textures. The GL backend (`glBackend.ts`) is the real one. */
export interface TextureBackend<T> {
  /** A plain-paper texture, shown while a page loads and for the pastedowns. */
  paper(paper: Rgb): T;
  /**
   * Uploads a page. A composed page is uploaded as is; a raw scan is first letterboxed onto
   * paper at height texH. Throws when the GPU refuses it.
   */
  page(page: DecodedPage, paper: Rgb, texH: number): T;
  /** Deletes a texture made by this backend. */
  free(tex: T): void;
}

/** What the cache needs from its owner. */
export interface PageTextureHost {
  /** The page box's aspect, height over width. Fixed for the book. */
  readonly boxAspect: number;
  /** The texture height for new uploads. It may change after a resize; cached textures keep theirs. */
  texHeight(): number;
  /** The page behind a slot, or null for a pastedown. */
  sourceOf(slot: number): PageSource | null;
  /** Slots that must not be evicted: the open spread and the turning leaf. */
  pinned(): Iterable<number>;
  /** A texture arrived. */
  onLoad(): void;
  /** A page failed to load or upload. It keeps showing paper and is not retried. */
  onError(slot: number, err: unknown): void;
}

// A cached texture and the height it was built at, so a bigger target can replace it later.
type Cached<T> = { tex: T; texH: number };
// A decode in flight: its generation's signal, the height it will compose to, and a promise that
// settles when the attempt is over, however it ended.
type Loading = { signal: AbortSignal; texH: number; done: Promise<void> };

/** LRU of page textures keyed by slot. */
export class PageTextures<T> {
  private readonly cache: LruCache<Cached<T>>;
  private readonly loading = new Map<number, Loading>();
  private readonly failed = new Set<number>();
  private abort = new AbortController();
  private paperTex: T;
  private paperColor: Rgb;
  private suspended = false;

  /** Builds the paper texture at once, so a backend failure throws from the constructor. */
  constructor(
    private readonly backend: TextureBackend<T>,
    private readonly decode: PageDecoder,
    private readonly host: PageTextureHost,
    paper: Rgb,
    capacity = TEX_CACHE,
  ) {
    this.cache = new LruCache<Cached<T>>(capacity, (c) => backend.free(c.tex));
    this.paperColor = paper;
    this.paperTex = backend.paper(paper);
  }

  /** The number of page textures held. */
  get size(): number {
    return this.cache.size;
  }

  /** Whether a slot has a texture. */
  has(slot: number): boolean {
    return this.cache.has(slot);
  }

  /** The slot's texture, or plain paper while it loads (and starts the load). */
  get(slot: number): T {
    const c = this.cache.get(slot);
    if (c) return c.tex;
    this.load(slot);
    return this.paperTex;
  }

  /** Starts decoding a slot unless it is cached, loading, failed, or a pastedown. */
  load(slot: number): void {
    if (this.cache.has(slot) || this.failed.has(slot)) return;
    this.start(slot, false);
  }

  /**
   * Rebuilds cached textures that are much smaller than the current target height, for example
   * after the canvas grew from its first tiny layout. The old texture stays on screen until the
   * new one lands, and a failed rebuild keeps the old one. Decodes in flight that would also land
   * too small are aborted; they start again on demand. Call it after a resize.
   */
  refresh(): void {
    if (this.suspended) return;
    const target = this.host.texHeight();
    const stale = slotsToReload(this.cache.snapshot().map(([slot, c]) => [slot, c.texH] as const), target);
    const staleLoads = [...this.loading.values()].some((l) => needsReload(l.texH, target));
    if (staleLoads) this.restartLoads();
    for (const slot of stale) this.start(slot, true);
  }

  // Decodes a slot. A rebuild (replacing) keeps the cached texture and drops its result if the
  // slot was evicted meanwhile; a failed rebuild is not an error, because a texture is on screen.
  private start(slot: number, replacing: boolean): void {
    const source = this.host.sourceOf(slot);
    if (this.suspended || !source || this.loading.has(slot)) return;
    const signal = this.abort.signal;
    const texH = this.host.texHeight();
    const target: ComposeTarget = { boxAspect: this.host.boxAspect, texH, paper: this.paperColor };
    const done = this.decode(source, target, signal)
      .then((page) => {
        if (signal.aborted || this.suspended || (replacing && !this.cache.has(slot))) return page.bitmap.close();
        if (replacing) this.replace(slot, page);
        else this.accept(slot, page);
      })
      .catch((err: unknown) => {
        if (!signal.aborted && !replacing) this.fail(slot, err);
      })
      .finally(() => {
        // A restart may have begun a newer load of this slot; only clear our own entry.
        if (this.loading.get(slot)?.signal === signal) this.loading.delete(slot);
      });
    this.loading.set(slot, { signal, texH, done });
  }

  /**
   * Starts any of the slots not yet loaded and resolves once each one is cached, has failed
   * (the host has been told), or has nothing to load. A load that a resize aborted is started
   * again, so the wait ends on a texture and not on a dropped decode. It stops waiting when the
   * optional signal aborts, when the cache is disposed, or while suspended. It never rejects.
   */
  async whenSettled(slots: readonly number[], signal?: AbortSignal): Promise<void> {
    while (!signal?.aborted && !this.abort.signal.aborted && !this.suspended) {
      const waits: Promise<void>[] = [];
      for (const slot of slots) {
        if (this.cache.has(slot) || this.failed.has(slot) || !this.host.sourceOf(slot)) continue;
        this.load(slot);
        const entry = this.loading.get(slot);
        if (entry) waits.push(entry.done);
      }
      if (waits.length === 0) return;
      await Promise.allSettled(waits);
    }
  }

  /**
   * Uploads a decoded page for a slot and tells the host. A failed upload leaves the slot on
   * paper and reports through the host's `onError`.
   */
  accept(slot: number, page: DecodedPage): void {
    try {
      this.upload(slot, page);
    } catch (err) {
      this.fail(slot, err);
      return;
    }
    this.host.onLoad();
  }

  // Swaps in a rebuilt texture. If the GPU refuses it the old texture stays, with no report: the
  // page is on screen and the next resize tries again.
  private replace(slot: number, page: DecodedPage): void {
    try {
      this.upload(slot, page);
    } catch {
      return;
    }
    this.host.onLoad();
  }

  /** Uploads a decoded page into the cache and closes its bitmap. Throws on a GPU error. */
  upload(slot: number, page: DecodedPage): void {
    try {
      const texH = this.host.texHeight();
      const tex = this.backend.page(page, this.paperColor, texH);
      this.cache.set(slot, { tex, texH: page.composed ? page.height : texH }, this.host.pinned());
    } finally {
      page.bitmap.close();
    }
  }

  /**
   * Rebuilds the paper texture and drops every cached page, because the letterbox band bakes the
   * paper colour into each page. In-flight loads are aborted and their bitmaps closed, because a
   * page composed in the worker carries the old colour; loads for the same slots start again on
   * demand with the new one. If the new paper texture cannot be built this throws and changes
   * nothing. While suspended there is no GPU to upload to: it only records the colour and clears
   * the cache, and `resume` builds the texture from that colour.
   */
  setPaper(paper: Rgb): void {
    if (this.suspended) {
      this.paperColor = paper;
      this.cache.clear();
      return;
    }
    const fresh = this.backend.paper(paper);
    this.paperColor = paper;
    this.backend.free(this.paperTex);
    this.paperTex = fresh;
    this.cache.clear();
    this.restartLoads();
  }

  /**
   * The WebGL context was lost. Drops every cached handle (GL ignores deletes on a lost context),
   * aborts the decodes in flight, and stops accepting loads until `resume`.
   */
  suspend(): void {
    this.suspended = true;
    this.cache.clear();
    this.restartLoads();
  }

  /** The context is back. Rebuilds the paper texture; pages reload on demand. */
  resume(): void {
    this.failed.clear();
    this.paperTex = this.backend.paper(this.paperColor);
    this.suspended = false;
  }

  /** Aborts loads and frees every texture. */
  dispose(): void {
    this.abort.abort();
    this.cache.clear();
    this.backend.free(this.paperTex);
  }

  // Aborts every decode in flight and starts a new generation. Their bitmaps are closed as they land.
  private restartLoads(): void {
    this.abort.abort();
    this.abort = new AbortController();
    this.loading.clear();
  }

  private fail(slot: number, err: unknown): void {
    // A failed page is not retried on every frame; it stays plain paper and is reported once.
    this.failed.add(slot);
    this.host.onError(slot, err);
  }
}
