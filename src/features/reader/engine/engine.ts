// Flipbook engine: mockup E's lit WebGL2 book with a curling page, driven by scan textures and a
// scene file. A plain class with no React. It owns pointer input on the canvas only; the chrome
// owns the keyboard. See docs/adr/0001-flipbook-engine.md.

import {
  clearSlot, createGeometryBuffer, writeLeafSlot, writeStackSlot,
} from "./corners";
import { decodePage, type DecodedPage, type PageDecoder, type PageSource } from "./decode";
import { makeBook, type Book } from "./geometry";
import { createGlBackend } from "./glBackend";
import { easeOut } from "./math";
import { PageTextures } from "./pageTextures";
import { PointerInput } from "./pointerInput";
import { BookRenderer, type Fade } from "./renderer";
import type { Scene } from "./scene";
import { pageOfSlot, spreadCount } from "./spreads";
import { EXTRA, TurnMachine, type TurnHost } from "./turnMachine";

/** The discrete state the chrome follows. Fires on change only, never per frame. */
export type EngineChange = { spread: number; spreads: number; turning: boolean; loaded: number };

/** Something went wrong. `page` is the 0-based scan that failed, or null for the engine itself. */
export type EngineError = { page: number | null; message: string; cause: unknown };

/** Options for `FlipbookEngine.create`. */
export type EngineOptions = {
  /** The book's scans in order, URLs from `pageUrl`. Needs at least one. */
  pages: PageSource[];
  scene: Scene;
  /** The spread to open, clamped into the book. */
  startSpread: number;
  /** A turn becomes a short cross-fade, and nothing pulses. */
  reducedMotion: boolean;
  onChange?: (state: EngineChange) => void;
  onError?: (error: EngineError) => void;
  /**
   * Called with the projected page corners while a leaf moves and once when the book settles.
   * The same buffer every time; copy what you keep. Layout in `corners.ts`.
   */
  onGeometry?: (buffer: Float32Array) => void;
  /** Page decoder. Defaults to the main-thread one; M1 chunk 2 passes the worker pipeline. */
  decoder?: PageDecoder;
};

/** What `create` returns. */
export interface FlipbookController {
  /** Turn to the next spread. A no-op at the end. */
  next(): void;
  /** Turn to the previous spread. A no-op at the start. */
  prev(): void;
  /** Open a spread. A distant one jumps next to it and turns the last step. */
  goTo(spread: number): void;
  /**
   * Restyle the book from a new scene. Page textures are rebuilt for the new paper colour. Never
   * throws, and reports failures through `onError`. If the new paper texture cannot be built, the
   * old scene stays whole. Layout runs after the scene is committed, so a layout failure leaves
   * the new scene applied with the previous camera.
   */
  setScene(scene: Scene): void;
  /** Switch between the curl and the cross-fade. Applies from the next turn. */
  setReducedMotion(reduced: boolean): void;
  /** Free everything. Safe to call twice. */
  dispose(): void;
}

const NO_WEBGL2 = "WebGL2 is not available in this webview. The reader needs it and has no fallback.";
const FADE_SECONDS = 0.2;

/** The page-turn engine. Use `FlipbookEngine.create`. */
export class FlipbookEngine implements FlipbookController {
  private readonly book: Book;
  private readonly renderer: BookRenderer;
  private textures!: PageTextures<WebGLTexture>;
  private machine!: TurnMachine;
  private input!: PointerInput;
  private readonly nLeaf: number;
  private scene: Scene;
  private fade: { lp: number; rp: number; el: number } | null = null;
  private readonly geometry = createGeometryBuffer();
  private geometryDue = true;
  private lastEmit = "";
  private dirty = true;
  private raf = 0;
  private last = performance.now();
  private disposed = false;
  private lost = false;
  private readonly cleanup: (() => void)[] = [];

  /**
   * Starts an engine on a canvas. Page 1 decodes first because its aspect fixes the book's page
   * box. Rejects when WebGL2 is missing, there are no pages, page 1 fails, or the signal aborts;
   * an aborted start leaves nothing behind, so a StrictMode double mount is safe.
   */
  static async create(canvas: HTMLCanvasElement, opts: EngineOptions, signal?: AbortSignal): Promise<FlipbookController> {
    const gl = canvas.getContext("webgl2", { antialias: true, alpha: false });
    if (!gl) throw new Error(NO_WEBGL2);
    if (!opts.pages.length) throw new Error("no pages to show");
    const sig = signal ?? new AbortController().signal;
    const decoder = opts.decoder ?? decodePage;
    const first = await decoder(opts.pages[0], sig);
    if (sig.aborted) {
      first.bitmap.close();
      sig.throwIfAborted();
    }
    let engine: FlipbookEngine | null = null;
    try {
      engine = new FlipbookEngine(gl, canvas, opts, first, decoder);
      engine.start(first);
      return engine;
    } catch (err) {
      // A failed start (shader error, GL error) frees what it built.
      first.bitmap.close();
      engine?.dispose();
      throw err;
    }
  }

  private constructor(
    private readonly gl: WebGL2RenderingContext,
    private readonly canvas: HTMLCanvasElement,
    private readonly opts: EngineOptions,
    first: DecodedPage,
    private readonly decoder: PageDecoder,
  ) {
    this.scene = opts.scene;
    this.nLeaf = spreadCount(opts.pages.length) - 1;
    this.book = makeBook(first.height / first.width, this.nLeaf);
    this.renderer = new BookRenderer(gl, canvas, this.book);
  }

  private start(first: DecodedPage) {
    const boxAspect = this.book.H / this.book.W;
    this.renderer.build();
    this.textures = new PageTextures(createGlBackend(this.gl, boxAspect), this.decoder, this.textureHost(), this.scene.material.paper);
    this.machine = new TurnMachine(this.book, this.nLeaf, this.turnHost(), this.opts.reducedMotion, this.opts.startSpread);
    this.input = new PointerInput(
      this.canvas, this.book, this.machine,
      { inverse: () => this.renderer.inverse, size: () => this.renderer.size },
      this.opts.reducedMotion,
    );
    this.textures.accept(1, first);
    this.listen();
    this.renderer.layout(this.scene);
    this.restStacks(this.machine.spread);
    this.prefetch();
    this.raf = requestAnimationFrame(this.frame);
    this.emit();
  }

  // ---------- public API ----------

  /** Turn to the next spread. A no-op at the end. */
  next() {
    this.machine.go(1);
  }

  /** Turn to the previous spread. A no-op at the start. */
  prev() {
    this.machine.go(-1);
  }

  /** Open a spread. A distant one jumps next to it and turns the last step. */
  goTo(spread: number) {
    this.machine.goTo(spread);
  }

  /**
   * Restyles the book from a new scene. The paper texture is rebuilt and every cached page is
   * dropped, because the letterbox band bakes `material.paper` into each page.
   */
  setScene(scene: Scene) {
    if (this.disposed) return;
    try {
      // Paper first: it throws without changing anything, so a failure keeps the old skin whole.
      this.textures.setPaper(scene.material.paper);
      this.scene = scene;
      // While the context is lost there is no GPU to lay out on; the restore does it.
      if (!this.lost) {
        this.renderer.layout(scene);
        this.prefetch();
      }
    } catch (err) {
      this.fail(null, "could not apply the new scene", err);
      return;
    }
    this.dirty = true;
    this.geometryDue = true;
  }

  /** Switches between the curl and the cross-fade. Applies from the next turn. */
  setReducedMotion(reduced: boolean) {
    this.machine.setReducedMotion(reduced);
    this.input.setReducedMotion(reduced);
  }

  /** Frees everything. Safe to call twice, and after a start that threw part way. */
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    for (const off of this.cleanup) off();
    this.input?.detach();
    this.textures?.dispose();
    this.renderer.release();
    // Do not call WEBGL_lose_context here: a StrictMode remount gets the same context back
    // from getContext, and a lost context would leave the remounted book blank.
  }

  // ---------- state ----------

  private emit() {
    if (!this.opts.onChange) return;
    const state: EngineChange = {
      spread: this.machine.spread,
      spreads: this.nLeaf + 1,
      turning: this.machine.turning || !!this.fade,
      loaded: this.textures.size,
    };
    const key = `${state.spread}|${state.turning}|${state.loaded}`;
    if (key === this.lastEmit) return;
    this.lastEmit = key;
    this.opts.onChange(state);
  }

  private fail(page: number | null, message: string, cause: unknown) {
    console.error(`[flipbook] ${message}`, cause);
    this.opts.onError?.({ page, message, cause });
  }

  private textureHost() {
    return {
      sourceOf: (slot: number) => {
        const p = pageOfSlot(slot, this.opts.pages.length);
        return p === null ? null : this.opts.pages[p];
      },
      pinned: () => this.pinnedSlots(),
      onLoad: () => {
        this.dirty = true;
        this.emit();
      },
      onError: (slot: number, err: unknown) => this.fail(slot - 1, `page ${slot - 1} failed to load`, err),
    };
  }

  private turnHost(): TurnHost {
    return {
      stacks: (Ln, Rn, lp, rp) => {
        this.renderer.setStacks(Ln, Rn, lp, rp);
        this.dirty = true;
      },
      rest: (spread) => this.restStacks(spread),
      changed: () => this.emit(),
      settled: () => {
        this.prefetch();
        this.geometryDue = true;
      },
      fade: (from) => {
        this.fade = { lp: 2 * from, rp: 2 * from + 1, el: 0 };
      },
    };
  }

  private pinnedSlots(): number[] {
    const ST = this.renderer.stacks;
    const out = ST ? [ST.lp, ST.rp] : [];
    out.push(...this.machine.turnSlots());
    if (this.fade) out.push(this.fade.lp, this.fade.rp);
    return out;
  }

  private restStacks(spread: number) {
    this.renderer.setStacks(spread + EXTRA, this.nLeaf - spread + EXTRA, 2 * spread, 2 * spread + 1);
    this.dirty = true;
  }

  private prefetch() {
    const k = this.machine.spread, n = 2 * this.nLeaf + 2;
    for (const s of [2 * k, 2 * k + 1, 2 * k - 1, 2 * k + 2, 2 * k + 3, 2 * k - 2, 2 * k + 4, 2 * k - 3])
      if (s >= 0 && s < n) this.textures.load(s);
  }

  // ---------- geometry for the text overlay and hotspots ----------

  private publishGeometry() {
    const ST = this.renderer.stacks;
    if (!ST || !this.opts.onGeometry) return;
    const [w, h] = this.renderer.bufferSize, t = { VP: this.renderer.viewProjection, w, h };
    const n = this.opts.pages.length, buf = this.geometry;
    writeStackSlot(buf, 0, pageOfSlot(ST.lp, n) ?? -1, ST.L.prof, -1, this.book, t);
    writeStackSlot(buf, 1, pageOfSlot(ST.rp, n) ?? -1, ST.R.prof, 1, this.book, t);
    const T = this.machine.turn;
    if (T) {
      const back = (T.dir > 0 ? T.t : 1 - T.t) >= 0.5;
      writeLeafSlot(buf, 2, pageOfSlot(2 * T.leaf + (back ? 2 : 1), n) ?? -1, this.renderer.leafVertices, back, t);
    } else {
      clearSlot(buf, 2);
    }
    this.geometryDue = false;
    this.opts.onGeometry(buf);
  }

  // ---------- input, resize, context loss ----------

  private listen() {
    const c = this.canvas;
    this.input.attach();
    const onLost = (e: Event) => this.onContextLost(e);
    const onRestored = () => this.onContextRestored();
    c.addEventListener("webglcontextlost", onLost);
    c.addEventListener("webglcontextrestored", onRestored);
    this.cleanup.push(() => {
      c.removeEventListener("webglcontextlost", onLost);
      c.removeEventListener("webglcontextrestored", onRestored);
    });
    let t = 0;
    const ro = new ResizeObserver(() => {
      clearTimeout(t);
      t = window.setTimeout(() => {
        if (this.disposed || this.lost) return;
        this.renderer.layout(this.scene);
        this.dirty = true;
        this.geometryDue = true;
      }, 60);
    });
    ro.observe(c);
    this.cleanup.push(() => {
      clearTimeout(t);
      ro.disconnect();
    });
  }

  // preventDefault is what asks the browser to fire webglcontextrestored later.
  private onContextLost(e: Event) {
    e.preventDefault();
    if (this.disposed) return;
    this.lost = true;
    cancelAnimationFrame(this.raf);
    this.textures.suspend();
    this.renderer.release();
  }

  private onContextRestored() {
    if (this.disposed) return;
    try {
      this.renderer.build();
      this.renderer.layout(this.scene);
      this.machine.reapply();
      this.textures.resume();
    } catch (err) {
      this.fail(null, "could not rebuild the scene after the graphics context was lost", err);
      return;
    }
    this.lost = false;
    this.last = performance.now();
    this.geometryDue = true;
    this.prefetch();
    this.emit();
    this.raf = requestAnimationFrame(this.frame);
  }

  // ---------- main loop ----------

  private stepFade(dt: number): Fade | null {
    const f = this.fade;
    if (!f) return null;
    f.el += dt;
    if (f.el >= FADE_SECONDS) {
      this.fade = null;
      this.geometryDue = true;
      this.emit();
      return null;
    }
    return { lp: f.lp, rp: f.rp, alpha: easeOut(f.el / FADE_SECONDS) };
  }

  private readonly frame = (now: number) => {
    if (this.disposed || this.lost) return;
    const dt = Math.min(0.05, (now - this.last) / 1000);
    this.last = now;
    // onChange and onGeometry run user code, which may dispose the engine mid-frame.
    const moving = this.machine.step(dt);
    const hadFade = !!this.fade;
    const fade = this.stepFade(dt);
    if (this.disposed || this.lost) return;
    const T = this.machine.turn;
    if (T) this.renderer.uploadLeaf(T);
    if (T || this.geometryDue) this.publishGeometry();
    if (this.disposed || this.lost) return;
    if (this.dirty || moving || hadFade) {
      this.renderer.render(this.scene, (slot) => this.textures.get(slot), T, fade);
      this.dirty = false;
    }
    if (!this.disposed && !this.lost) this.raf = requestAnimationFrame(this.frame);
  };
}
