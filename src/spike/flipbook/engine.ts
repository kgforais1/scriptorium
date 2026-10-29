// Flipbook engine: mockup E's lit WebGL2 book with a curling page, driven by scan textures and a
// scene file. Spike code. See docs/adr/0001-flipbook-engine.md for what M1 keeps and rewrites.

import {
  NV, LEAF_FLOATS, boardMesh, gutterLift, leafIndices, makeBook, ribbonMesh, stackMeshes, tableMesh, updateLeaf,
  type Book, type Profile, type Turn,
} from "./geometry";
import { drawMesh, freeMesh, fullscreenTriangle, mesh, meshFrom, program, type Mesh, type Program } from "./gl";
import {
  IDENTITY, PI, clamp, easeInOut, easeOut, invert, lerp, lookAt, multiply, perspective, type Mat4, type Vec3,
} from "./math";
import { decodePage, type DecodedPage } from "./pages";
import type { Scene } from "./scene";
import { FS_BAKE, VS_FULL, VS_LIT, applySceneUniforms, fsLit } from "./shaders";
import { cornerAt, dragProgress, pickLocal, turnCompletes, type Drag } from "./gesture";
import { PageTextures, type TextureStats } from "./textures";

export type EngineState = { spread: number; spreads: number; turning: boolean; loaded: number; stats: EngineStats };
export type EngineStats = TextureStats;
export type EngineOptions = {
  pages: string[];
  scene: Scene;
  onState?: (s: EngineState) => void;
  onPageError?: (page: number, err: unknown) => void;
};

const EXTRA = 7; // visual padding leaves under each block
const INSETS = { top: 24, bot: 88, side: 28 };

type Stacks = { L: { top: Mesh; edge: Mesh; prof: Profile }; R: { top: Mesh; edge: Mesh; prof: Profile }; lp: number; rp: number };

export class FlipbookEngine {
  private gl: WebGL2RenderingContext;
  private book: Book;
  private nSlots: number;
  private nLeaf: number;
  // Assigned in start(); dispose() tolerates a start that threw part way.
  private lit!: Program;
  private bakeProg!: Program;
  private M!: { board: Mesh; table: Mesh; ribbon: Mesh; ribbonSh: Mesh; leaf: Mesh; fsTri: WebGLVertexArrayObject; fsBuf: WebGLBuffer };
  private leafV = new Float32Array(LEAF_FLOATS);
  private foot = new Float32Array(NV * 4);
  private bakeTex: WebGLTexture | null = null;
  private bakeFB: WebGLFramebuffer | null = null;
  private pages!: PageTextures;
  private cam = { VP: IDENTITY, inv: IDENTITY, eye: [0, 0, 5] as Vec3, k: 1 };
  private cssW = 1;
  private cssH = 1;
  private K = 0;
  private turn: Turn | null = null;
  private queue = 0;
  private ST: Stacks | null = null;
  private drag: Drag | null = null;
  private dirty = true;
  private raf = 0;
  private last = performance.now();
  private disposed = false;
  private cleanup: (() => void)[] = [];
  private reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // The first page is decoded before the engine exists, because its aspect fixes the book size.
  static async create(canvas: HTMLCanvasElement, opts: EngineOptions, signal: AbortSignal): Promise<FlipbookEngine> {
    if (!opts.pages.length) throw new Error("no pages to show");
    const first = await decodePage(opts.pages[0], signal);
    if (signal.aborted) {
      first.bitmap.close();
      signal.throwIfAborted();
    }
    let engine: FlipbookEngine | null = null;
    try {
      engine = new FlipbookEngine(canvas, opts, first);
      engine.start(first);
      return engine;
    } catch (err) {
      // A failed start (no WebGL2, shader error) may never reach upload, and frees what it built.
      first.bitmap.close();
      engine?.dispose();
      throw err;
    }
  }

  private constructor(private canvas: HTMLCanvasElement, private opts: EngineOptions, first: DecodedPage) {
    const gl = canvas.getContext("webgl2", { antialias: true, alpha: false });
    if (!gl) throw new Error("WebGL2 is not available in this webview");
    this.gl = gl;
    // Slot 0 is the blank front pastedown, slot i is page i-1, and the last slot is the back pastedown.
    this.nSlots = opts.pages.length + 1 + ((opts.pages.length + 1) % 2);
    this.nLeaf = this.nSlots / 2 - 1;
    this.book = makeBook(first.height / first.width, this.nLeaf);
  }

  private start(first: DecodedPage) {
    const gl = this.gl, b = this.book, opts = this.opts;
    this.lit = program(gl, VS_LIT, fsLit({ W: b.W, H: b.H, MRG: b.MRG, NV }));
    this.bakeProg = program(gl, VS_FULL, FS_BAKE);
    const [fsTri, fsBuf] = fullscreenTriangle(gl);
    this.M = {
      board: meshFrom(gl, boardMesh(b)),
      table: meshFrom(gl, tableMesh()),
      ribbon: meshFrom(gl, ribbonMesh(b, 0.026)),
      ribbonSh: meshFrom(gl, ribbonMesh(b, 0.05)),
      leaf: mesh(gl, { V: this.leafV, I: leafIndices() }, true),
      fsTri,
      fsBuf,
    };
    const onLoad = () => {
      this.dirty = true;
      this.emit();
    };
    const onError = (slot: number, err: unknown) => this.opts.onPageError?.(slot - 1, err);
    this.pages = new PageTextures(gl, b.H, opts.scene.material.paper, (s) => this.slotUrl(s), () => this.pinnedSlots(), onLoad, onError);
    this.pages.upload(1, first);
    this.listen();
    this.layout();
    this.restStacks();
    this.prefetch();
    this.raf = requestAnimationFrame(this.frame);
    this.emit();
  }

  // ---------- public API ----------

  next() {
    this.go(1);
  }

  prev() {
    this.go(-1);
  }

  goTo(spread: number) {
    const s = clamp(Math.round(spread), 0, this.nLeaf);
    if (s === this.K || this.turn) return;
    this.queue = 0;
    const dir = Math.sign(s - this.K) as 1 | -1;
    if (Math.abs(s - this.K) > 1) {
      this.K = s - dir;
      this.restStacks();
    }
    this.go(dir, 0.9);
  }

  // Restyles the book from a new scene file. Page textures are recomposed because the
  // letterbox band uses the skin's paper colour.
  setScene(scene: Scene) {
    this.opts = { ...this.opts, scene };
    this.pages.setPaper(scene.material.paper);
    this.layout();
    this.prefetch();
    this.dirty = true;
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    for (const off of this.cleanup) off();
    const gl = this.gl;
    this.pages?.dispose();
    gl.deleteTexture(this.bakeTex);
    gl.deleteFramebuffer(this.bakeFB);
    this.freeStacks();
    const M = this.M;
    if (M) for (const m of [M.board, M.table, M.ribbon, M.ribbonSh, M.leaf]) freeMesh(gl, m);
    gl.deleteVertexArray(M?.fsTri ?? null);
    gl.deleteBuffer(M?.fsBuf ?? null);
    gl.deleteProgram(this.lit?.p ?? null);
    gl.deleteProgram(this.bakeProg?.p ?? null);
    // Do not call WEBGL_lose_context here: a StrictMode remount gets the same context back
    // from getContext, and a lost context would leave the remounted book blank.
  }

  // ---------- state ----------

  private emit() {
    this.opts.onState?.({
      spread: this.K,
      spreads: this.nLeaf + 1,
      turning: !!this.turn && this.turn.kind !== "peek",
      loaded: this.pages.size,
      stats: { ...this.pages.stats },
    });
  }

  private slotUrl(slot: number): string | null {
    const i = slot - 1;
    return i >= 0 && i < this.opts.pages.length ? this.opts.pages[i] : null;
  }

  private pinnedSlots(): number[] {
    const out = this.ST ? [this.ST.lp, this.ST.rp] : [];
    if (this.turn) out.push(2 * this.turn.leaf + 1, 2 * this.turn.leaf + 2);
    return out;
  }

  private prefetch() {
    const k = this.K;
    for (const s of [2 * k, 2 * k + 1, 2 * k - 1, 2 * k + 2, 2 * k + 3, 2 * k - 2, 2 * k + 4, 2 * k - 3])
      if (s >= 0 && s < this.nSlots) this.pages.load(s);
  }

  // ---------- stacks & camera ----------

  private freeStacks() {
    if (!this.ST) return;
    for (const m of [this.ST.L.top, this.ST.L.edge, this.ST.R.top, this.ST.R.edge]) freeMesh(this.gl, m);
    this.ST = null;
  }

  private setStacks(Ln: number, Rn: number, lp: number, rp: number) {
    this.freeStacks();
    const gl = this.gl, b = this.book, L = stackMeshes(b, -1, Ln), R = stackMeshes(b, 1, Rn);
    this.ST = {
      L: { top: meshFrom(gl, L.top), edge: meshFrom(gl, L.edge), prof: L.prof },
      R: { top: meshFrom(gl, R.top), edge: meshFrom(gl, R.edge), prof: R.prof },
      lp,
      rp,
    };
    this.dirty = true;
  }

  private restStacks() {
    this.setStacks(this.K + EXTRA, this.nLeaf - this.K + EXTRA, 2 * this.K, 2 * this.K + 1);
  }

  private layout() {
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    this.cssW = Math.max(1, this.canvas.clientWidth);
    this.cssH = Math.max(1, this.canvas.clientHeight);
    this.canvas.width = Math.round(this.cssW * dpr);
    this.canvas.height = Math.round(this.cssH * dpr);
    const b = this.book, fov = (this.opts.scene.camera.fovDegrees * PI) / 180;
    const availW = this.cssW - 2 * INSETS.side, availH = this.cssH - INSETS.top - INSETS.bot;
    const hx = b.W + b.MRG + 0.027, hy = b.H / 2 + b.MRG + 0.017;
    const k = Math.max(1e-3, Math.min(availW / (2 * hx), availH / (2 * hy)));
    const D = this.cssH / (2 * k * Math.tan(fov / 2)), sy = INSETS.top + availH / 2;
    const tgt: Vec3 = [0, (sy - this.cssH / 2) / k, 0];
    this.cam.eye = [tgt[0], tgt[1], D];
    this.cam.k = k;
    this.cam.VP = multiply(perspective(fov, this.cssW / this.cssH, 0.3, D + 10), lookAt(this.cam.eye, tgt, [0, 1, 0]));
    this.cam.inv = invert(this.cam.VP);
    this.bake();
    this.dirty = true;
  }

  private bake() {
    const gl = this.gl, s = this.opts.scene.material.desk;
    if (this.bakeTex) gl.deleteTexture(this.bakeTex);
    if (this.bakeFB) gl.deleteFramebuffer(this.bakeFB);
    this.bakeTex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.bakeTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, this.canvas.width, this.canvas.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    this.bakeFB = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.bakeFB);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, this.bakeTex, 0);
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.disable(gl.DEPTH_TEST);
    gl.disable(gl.BLEND);
    const u = this.bakeProg.u;
    gl.useProgram(this.bakeProg.p);
    gl.uniformMatrix4fv(u.uInvVP, false, this.cam.inv);
    gl.uniform2f(u.uRes, this.canvas.width, this.canvas.height);
    gl.uniform3fv(u.uDeskLight, s.light);
    gl.uniform3fv(u.uDeskMid, s.mid);
    gl.uniform3fv(u.uDeskDark, s.dark);
    gl.bindVertexArray(this.M.fsTri);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  // ---------- rendering ----------

  private bindTex(a: WebGLTexture, b?: WebGLTexture) {
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, a);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, b || a);
  }

  private render() {
    const gl = this.gl, u = this.lit.u, ST = this.ST!, bg = this.opts.scene.background.srgb;
    const W = this.canvas.width, H = this.canvas.height;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, W, H);
    gl.clearColor(bg[0], bg[1], bg[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    gl.disable(gl.CULL_FACE);
    gl.disable(gl.BLEND);
    gl.depthMask(true);
    gl.useProgram(this.lit.p);
    gl.uniform1i(u.uTexA, 0);
    gl.uniform1i(u.uTexB, 1);
    gl.uniformMatrix4fv(u.uVP, false, this.cam.VP);
    gl.uniform3fv(u.uCam, this.cam.eye);
    applySceneUniforms(gl, u, this.opts.scene);
    gl.uniform2f(u.uRes, W, H);
    gl.uniformMatrix4fv(u.uBookInv, false, IDENTITY);
    gl.uniform1f(u.uShadow, this.turn ? 1 : 0);
    gl.uniform4fv(u.uFoot, this.foot);
    gl.uniform1f(u.uFlag, 0);
    gl.uniform1f(u.uRibbonLen, this.M.ribbon.len!);
    const mat = (m: number, model?: Mat4) => {
      gl.uniform1i(u.uMat, m);
      if (model) gl.uniformMatrix4fv(u.uModel, false, model);
    };
    mat(7, IDENTITY);
    gl.uniform2f(u.uTexel, 1 / W, 1 / H);
    this.bindTex(this.bakeTex!);
    drawMesh(gl, this.M.table);
    // ribbon's soft shadow on the desk
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.depthMask(false);
    mat(9, new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0.016, -0.018, 0.0006, 1]));
    gl.uniform1f(u.uAlpha, 0.45);
    drawMesh(gl, this.M.ribbonSh);
    gl.disable(gl.BLEND);
    gl.depthMask(true);
    mat(2, IDENTITY);
    drawMesh(gl, this.M.board);
    mat(0);
    this.bindTex(this.pages.get(ST.lp));
    drawMesh(gl, ST.L.top);
    this.bindTex(this.pages.get(ST.rp));
    drawMesh(gl, ST.R.top);
    mat(1);
    drawMesh(gl, ST.L.edge);
    drawMesh(gl, ST.R.edge);
    mat(3);
    drawMesh(gl, this.M.ribbon);
    if (this.turn) {
      gl.uniform1f(u.uShadow, 0);
      gl.uniform1f(u.uFlag, 1);
      mat(0);
      this.bindTex(this.pages.get(2 * this.turn.leaf + 1), this.pages.get(2 * this.turn.leaf + 2));
      drawMesh(gl, this.M.leaf);
    }
  }

  private uploadLeaf() {
    const gl = this.gl;
    updateLeaf(this.book, this.turn!, this.leafV, this.foot);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.M.leaf.vb);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.leafV);
  }

  // ---------- turning ----------

  private beginTurn(dir: 1 | -1, kind: Turn["kind"], cv?: number): Turn | null {
    if (this.turn) return this.turn;
    const K = this.K;
    if (dir > 0 && K >= this.nLeaf) return null;
    if (dir < 0 && K <= 0) return null;
    const leaf = dir > 0 ? K : K - 1;
    const Ln = dir > 0 ? K + EXTRA : K - 1 + EXTRA, Rn = dir > 0 ? this.nLeaf - K - 1 + EXTRA : this.nLeaf - K + EXTRA;
    this.setStacks(Ln, Rn, dir > 0 ? 2 * K : 2 * K - 2, dir > 0 ? 2 * K + 3 : 2 * K + 1);
    const b = this.book;
    this.turn = {
      dir, leaf, t: dir > 0 ? 0 : 1, kind, cv: cv ?? 0.94, A0: 0.55, pk: 0, pkT: 1, lift: 0, cancel: 0,
      gA: dir > 0 ? gutterLift(b, Rn + 1) : gutterLift(b, Ln + 1),
      gB: dir > 0 ? gutterLift(b, Ln + 1) : gutterLift(b, Rn + 1),
      anim: null,
    };
    this.emit();
    return this.turn;
  }

  private animateTo(target: number, dur: number, ease: (x: number) => number) {
    const T = this.turn!;
    if (T.kind === "peek") T.lift = T.pk;
    T.kind = "anim";
    const tp = T.dir > 0 ? T.t : 1 - T.t, toStart = T.dir > 0 ? target === 0 : target === 1;
    T.cancel = toStart ? Math.max(tp, 0.001) : 0;
    T.anim = { t0: T.t, t1: target, el: 0, dur: this.reducedMotion ? Math.min(dur, 0.3) : dur, ease };
    this.emit();
  }

  private finishTurn() {
    const T = this.turn!;
    this.turn = null;
    const landedLeft = T.t >= 0.5;
    if (T.dir > 0 && landedLeft) this.K++;
    else if (T.dir < 0 && !landedLeft) this.K--;
    this.restStacks();
    this.prefetch();
    this.emit();
    this.drainQueue();
  }

  private drainQueue() {
    if (this.queue === 0) return;
    const d = Math.sign(this.queue) as 1 | -1;
    // A step past either end of the book is dropped, along with the rest of the queue.
    if ((d > 0 && this.K >= this.nLeaf) || (d < 0 && this.K <= 0)) {
      this.queue = 0;
      return;
    }
    this.queue -= d;
    this.go(d, 0.75);
  }

  // Navigation during an animation (or a settling corner preview) is queued and applied when
  // it ends. Navigation during an active drag is dropped.
  private go(dir: 1 | -1, dur = 1.05) {
    const T = this.turn;
    if (T) {
      if (this.drag) return;
      if (T.kind === "peek" && T.dir === dir) {
        T.A0 = 1.3;
        this.animateTo(dir > 0 ? 1 : 0, dur, easeInOut);
      } else {
        if (T.kind === "peek") T.pkT = 0;
        this.queue = clamp(this.queue + dir, -2, 2);
      }
      return;
    }
    if (!this.beginTurn(dir, "anim")) return;
    this.animateTo(dir > 0 ? 1 : 0, dur, easeInOut);
  }

  // ---------- input ----------

  private local(e: PointerEvent) {
    const r = this.canvas.getBoundingClientRect();
    return pickLocal(this.cam.inv, this.cssW, this.cssH, e.clientX - r.left, e.clientY - r.top);
  }

  private onMove = (e: PointerEvent) => {
    const l = this.local(e);
    if (this.drag) this.dragMove(e, l, this.drag);
    // Hover peek is a mouse-only polish; touch and pen get the same turn by dragging.
    else if (e.pointerType === "mouse") this.hoverMove(l);
  };

  private dragMove(e: PointerEvent, l: [number, number], d: Drag) {
    const T = this.turn;
    if (!T || e.pointerId !== d.id) return;
    if (Math.abs(e.clientX - d.sx) > 4 || Math.abs(e.clientY - d.sy) > 4) d.moved = true;
    if (!d.moved) return;
    if (T.kind === "peek") T.lift = T.pk;
    if (T.kind !== "drag") {
      T.kind = "drag";
      this.emit();
    }
    T.anim = null;
    T.cancel = 0;
    const now = performance.now(), dt = Math.max(1, now - d.lt);
    d.vx = lerp(d.vx, ((l[0] - d.lx) / dt) * 1000, 0.5);
    d.lx = l[0];
    d.lt = now;
    T.t = dragProgress(T.dir, d.x0, l[0]);
  }

  private hoverMove(l: [number, number]) {
    const c = cornerAt(this.book, l), h2 = this.book.H / 2;
    const onPage = Math.abs(l[1]) < h2 && Math.abs(l[0]) < 1.05 && Math.abs(l[0]) > 0.04;
    this.canvas.style.cursor = c || onPage ? "grab" : "default";
    if (c && !this.turn) {
      const T = this.beginTurn(c.side > 0 ? 1 : -1, "peek", c.cv);
      if (T) T.pkT = 1;
    } else if (this.turn?.kind === "peek") {
      // Once navigation has queued a step, the peek must settle; hovering cannot re-arm it.
      this.turn.pkT = this.queue === 0 && c && this.turn.dir === c.side ? 1 : 0;
    }
  }

  private onLeave = () => {
    if (this.turn?.kind === "peek") this.turn.pkT = 0;
  };

  private onDown = (e: PointerEvent) => {
    if (e.button !== 0 || this.drag) return;
    const l = this.local(e), h2 = this.book.H / 2;
    if (Math.abs(l[1]) > h2 + 0.06 || Math.abs(l[0]) > 1.08 || Math.abs(l[0]) < 0.03) return;
    const dir = l[0] > 0 ? 1 : -1;
    if (this.turn && !(this.turn.kind === "peek" && this.turn.dir === dir)) return;
    const vn = clamp((h2 - l[1]) / this.book.H, 0, 1);
    const T = this.turn || this.beginTurn(dir, "drag", vn);
    if (!T) return;
    if (T.kind === "peek") T.A0 = 1.3;
    else {
      T.cv = vn;
      T.A0 = Math.abs(vn - 0.5) > 0.32 ? 1.3 : 0.45;
    }
    this.drag = { id: e.pointerId, sx: e.clientX, sy: e.clientY, x0: l[0], lx: l[0], lt: performance.now(), vx: 0, moved: false };
    this.canvas.setPointerCapture(e.pointerId);
    this.canvas.style.cursor = "grabbing";
    e.preventDefault();
  };

  // Release, pointercancel, and lost capture all settle the page, so the book never sticks mid-turn.
  private onRelease = (e: PointerEvent) => {
    const d = this.drag;
    if (!d || e.pointerId !== d.id) return;
    this.drag = null;
    this.canvas.style.cursor = "grab";
    const T = this.turn;
    if (!T) return;
    if (!d.moved) {
      if (e.type === "pointerup") this.animateTo(T.dir > 0 ? 1 : 0, 1.05, easeInOut);
      else this.animateTo(T.dir > 0 ? 0 : 1, 0.3, easeOut);
      return;
    }
    const target = (T.dir > 0) === turnCompletes(T, d, e.type === "pointerup") ? 1 : 0;
    this.animateTo(target, 0.3 + 0.55 * Math.abs(target - T.t), easeOut);
  };


  private onKey = (e: KeyboardEvent) => {
    if (e.defaultPrevented || e.target instanceof HTMLInputElement) return;
    if (e.key === "ArrowRight" || e.key === "PageDown") {
      e.preventDefault();
      this.go(1);
    } else if (e.key === "ArrowLeft" || e.key === "PageUp") {
      e.preventDefault();
      this.go(-1);
    }
  };

  private listen() {
    const c = this.canvas;
    const on = <K extends keyof HTMLElementEventMap>(t: K, f: (e: HTMLElementEventMap[K]) => void) => {
      c.addEventListener(t, f);
      this.cleanup.push(() => c.removeEventListener(t, f));
    };
    on("pointermove", this.onMove);
    on("pointerleave", this.onLeave);
    on("pointerdown", this.onDown);
    on("pointerup", this.onRelease);
    on("pointercancel", this.onRelease);
    on("lostpointercapture", this.onRelease);
    window.addEventListener("keydown", this.onKey);
    this.cleanup.push(() => window.removeEventListener("keydown", this.onKey));
    let t = 0;
    const ro = new ResizeObserver(() => {
      clearTimeout(t);
      t = window.setTimeout(() => !this.disposed && this.layout(), 60);
    });
    ro.observe(c);
    this.cleanup.push(() => {
      clearTimeout(t);
      ro.disconnect();
    });
  }

  // ---------- main loop ----------

  private frame = (now: number) => {
    if (this.disposed) return;
    const dt = Math.min(0.05, (now - this.last) / 1000);
    this.last = now;
    let anim = false;
    const T = this.turn;
    if (T) {
      if (T.kind === "peek") {
        T.pk += (T.pkT - T.pk) * (1 - Math.exp(-dt * 9));
        anim = true;
        if (T.pkT === 0 && T.pk < 0.01) {
          this.turn = null;
          this.restStacks();
          this.emit();
          this.drainQueue();
        }
      } else {
        if (T.lift < 1) {
          T.lift = Math.min(1, T.lift + (1 - T.lift) * (1 - Math.exp(-dt * 12)) + dt * 0.2);
          anim = true;
        }
        if (T.anim) {
          const A = T.anim;
          A.el += dt;
          const k = clamp(A.el / A.dur, 0, 1);
          T.t = lerp(A.t0, A.t1, A.ease(k));
          anim = true;
          if (k >= 1) {
            T.t = A.t1;
            T.anim = null;
            this.finishTurn();
          }
        }
        if (T.kind === "drag") anim = true;
      }
    }
    if (this.turn) this.uploadLeaf();
    if (this.dirty || anim) {
      this.render();
      this.dirty = false;
    }
    this.raf = requestAnimationFrame(this.frame);
  };
}
