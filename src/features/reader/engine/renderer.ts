// The GL side of the book: programs, static meshes, the baked desk, the page blocks, the
// turning leaf, and the draw. It owns no input and no turn logic. Excluded from coverage
// because it needs a GPU.

import { computeCamera, type Camera } from "./camera";
import {
  NV, LEAF_FLOATS, boardMesh, leafIndices, ribbonMesh, stackMeshes, tableMesh, updateLeaf,
  type Book, type Profile, type Turn,
} from "./geometry";
import { drawMesh, freeMesh, fullscreenTriangle, mesh, meshFrom, program, type Mesh, type Program } from "./gl";
import { IDENTITY, type Mat4 } from "./math";
import type { Scene } from "./scene";
import { FS_BAKE, VS_FULL, VS_LIT, applySceneUniforms, fsLit } from "./shaders";

type Block = { top: Mesh; edge: Mesh; prof: Profile };
/** The page blocks on show: a mesh pair per side, and the slot on top of each. */
export type Stacks = { L: Block; R: Block; lp: number; rp: number };
/** Reduced-motion cross-fade: the slots fading out, and the opacity of the new spread. */
export type Fade = { lp: number; rp: number; alpha: number };
type Statics = { board: Mesh; table: Mesh; ribbon: Mesh; ribbonSh: Mesh; leaf: Mesh; fsTri: WebGLVertexArrayObject; fsBuf: WebGLBuffer };

const RIBBON_SHADOW = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0.016, -0.018, 0.0006, 1]);

/** Draws the book scene. `build` must run before anything else, and again after a context restore. */
export class BookRenderer {
  /** The page blocks on show. Null until `setStacks`. */
  stacks: Stacks | null = null;
  private lit: Program | null = null;
  private bakeProg: Program | null = null;
  private M: Statics | null = null;
  private readonly leafV = new Float32Array(LEAF_FLOATS);
  private readonly foot = new Float32Array(NV * 4);
  private bakeTex: WebGLTexture | null = null;
  private bakeFB: WebGLFramebuffer | null = null;
  private cam: Camera = { VP: IDENTITY, inv: IDENTITY, eye: [0, 0, 5], k: 1 };
  private cssW = 1;
  private cssH = 1;

  /** `book` fixes the geometry, which is baked into the shaders. */
  constructor(
    private readonly gl: WebGL2RenderingContext,
    private readonly canvas: HTMLCanvasElement,
    private readonly book: Book,
  ) {}

  /** The inverse view-projection, for picking. */
  get inverse(): Mat4 {
    return this.cam.inv;
  }

  /** The canvas size in CSS pixels. */
  get size(): [number, number] {
    return [this.cssW, this.cssH];
  }

  /** The drawing-buffer size in pixels. */
  get bufferSize(): [number, number] {
    return [this.canvas.width, this.canvas.height];
  }

  /** The view-projection matrix. */
  get viewProjection(): Mat4 {
    return this.cam.VP;
  }

  /** Compiles programs and uploads the static meshes. Frees what it made if a step throws. */
  build(): void {
    const gl = this.gl, b = this.book;
    try {
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
    } catch (err) {
      this.release();
      throw err;
    }
  }

  /** Frees every GL object. Safe after a partial `build` and on a lost context. */
  release(): void {
    const gl = this.gl, M = this.M;
    gl.deleteTexture(this.bakeTex);
    gl.deleteFramebuffer(this.bakeFB);
    this.bakeTex = null;
    this.bakeFB = null;
    this.freeStacks();
    if (M) {
      for (const m of [M.board, M.table, M.ribbon, M.ribbonSh, M.leaf]) freeMesh(gl, m);
      gl.deleteVertexArray(M.fsTri);
      gl.deleteBuffer(M.fsBuf);
    }
    gl.deleteProgram(this.lit?.p ?? null);
    gl.deleteProgram(this.bakeProg?.p ?? null);
    this.M = null;
    this.lit = null;
    this.bakeProg = null;
  }

  /** Sizes the canvas to its element, rebuilds the camera, and re-bakes the desk. */
  layout(scene: Scene): void {
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    this.cssW = Math.max(1, this.canvas.clientWidth);
    this.cssH = Math.max(1, this.canvas.clientHeight);
    this.canvas.width = Math.round(this.cssW * dpr);
    this.canvas.height = Math.round(this.cssH * dpr);
    this.cam = computeCamera(this.cssW, this.cssH, this.book, scene.camera.fovDegrees);
    this.bake(scene);
  }

  /** Builds the page blocks. Ln and Rn are the leaf counts; lp and rp the slots on top. */
  setStacks(Ln: number, Rn: number, lp: number, rp: number): void {
    this.freeStacks();
    const gl = this.gl, b = this.book, L = stackMeshes(b, -1, Ln), R = stackMeshes(b, 1, Rn);
    this.stacks = {
      L: { top: meshFrom(gl, L.top), edge: meshFrom(gl, L.edge), prof: L.prof },
      R: { top: meshFrom(gl, R.top), edge: meshFrom(gl, R.edge), prof: R.prof },
      lp,
      rp,
    };
  }

  /** The turning leaf's vertex grid, valid after `uploadLeaf`. */
  get leafVertices(): Float32Array {
    return this.leafV;
  }

  /** Recomputes the turning leaf and uploads it. */
  uploadLeaf(turn: Turn): void {
    const gl = this.gl;
    updateLeaf(this.book, turn, this.leafV, this.foot);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.M!.leaf.vb);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.leafV);
  }

  /** Draws one frame. `tex` returns the texture for a slot (paper while it loads). */
  render(scene: Scene, tex: (slot: number) => WebGLTexture, turn: Turn | null, fade: Fade | null): void {
    const gl = this.gl, u = this.lit!.u, ST = this.stacks!, bg = scene.background.srgb;
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
    gl.useProgram(this.lit!.p);
    gl.uniform1i(u.uTexA, 0);
    gl.uniform1i(u.uTexB, 1);
    gl.uniformMatrix4fv(u.uVP, false, this.cam.VP);
    gl.uniform3fv(u.uCam, this.cam.eye);
    applySceneUniforms(gl, u, scene);
    gl.uniform2f(u.uRes, W, H);
    gl.uniformMatrix4fv(u.uBookInv, false, IDENTITY);
    gl.uniform1f(u.uShadow, turn ? 1 : 0);
    gl.uniform4fv(u.uFoot, this.foot);
    gl.uniform1f(u.uFlag, 0);
    gl.uniform1f(u.uRibbonLen, this.M!.ribbon.len!);
    this.drawDesk(u, W, H);
    this.drawBook(tex, ST, fade);
    if (turn) this.drawLeaf(u, tex, turn);
  }

  private mat(m: number, model?: Mat4): void {
    const gl = this.gl, u = this.lit!.u;
    gl.uniform1i(u.uMat, m);
    if (model) gl.uniformMatrix4fv(u.uModel, false, model);
  }

  private bindTex(a: WebGLTexture, b?: WebGLTexture): void {
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, a);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, b || a);
  }

  private drawDesk(u: Program["u"], W: number, H: number): void {
    const gl = this.gl, M = this.M!;
    this.mat(7, IDENTITY);
    gl.uniform2f(u.uTexel, 1 / W, 1 / H);
    this.bindTex(this.bakeTex!);
    drawMesh(gl, M.table);
    // ribbon's soft shadow on the desk
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.depthMask(false);
    this.mat(9, RIBBON_SHADOW);
    gl.uniform1f(u.uAlpha, 0.45);
    drawMesh(gl, M.ribbonSh);
    gl.disable(gl.BLEND);
    gl.depthMask(true);
  }

  private drawBook(tex: (slot: number) => WebGLTexture, ST: Stacks, fade: Fade | null): void {
    const gl = this.gl, M = this.M!;
    this.mat(2, IDENTITY);
    drawMesh(gl, M.board);
    this.mat(0);
    this.bindTex(tex(fade ? fade.lp : ST.lp));
    drawMesh(gl, ST.L.top);
    this.bindTex(tex(fade ? fade.rp : ST.rp));
    drawMesh(gl, ST.R.top);
    if (fade) this.drawFadeIn(tex, ST, fade);
    this.mat(1);
    drawMesh(gl, ST.L.edge);
    drawMesh(gl, ST.R.edge);
    this.mat(3);
    drawMesh(gl, M.ribbon);
  }

  // Reduced motion: lay the new spread over the old one with a constant blend alpha. The pages
  // are the same geometry, so the second draw passes the LEQUAL depth test.
  private drawFadeIn(tex: (slot: number) => WebGLTexture, ST: Stacks, fade: Fade): void {
    const gl = this.gl;
    gl.enable(gl.BLEND);
    gl.blendColor(0, 0, 0, fade.alpha);
    gl.blendFunc(gl.CONSTANT_ALPHA, gl.ONE_MINUS_CONSTANT_ALPHA);
    gl.depthMask(false);
    this.bindTex(tex(ST.lp));
    drawMesh(gl, ST.L.top);
    this.bindTex(tex(ST.rp));
    drawMesh(gl, ST.R.top);
    gl.disable(gl.BLEND);
    gl.depthMask(true);
  }

  private drawLeaf(u: Program["u"], tex: (slot: number) => WebGLTexture, T: Turn): void {
    const gl = this.gl;
    gl.uniform1f(u.uShadow, 0);
    gl.uniform1f(u.uFlag, 1);
    this.mat(0);
    this.bindTex(tex(2 * T.leaf + 1), tex(2 * T.leaf + 2));
    drawMesh(gl, this.M!.leaf);
  }

  private freeStacks(): void {
    const ST = this.stacks;
    if (!ST) return;
    for (const m of [ST.L.top, ST.L.edge, ST.R.top, ST.R.edge]) freeMesh(this.gl, m);
    this.stacks = null;
  }

  private bake(scene: Scene): void {
    const gl = this.gl, s = scene.material.desk, w = this.canvas.width, h = this.canvas.height;
    gl.deleteTexture(this.bakeTex);
    gl.deleteFramebuffer(this.bakeFB);
    this.bakeTex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.bakeTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    this.bakeFB = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.bakeFB);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, this.bakeTex, 0);
    gl.viewport(0, 0, w, h);
    gl.disable(gl.DEPTH_TEST);
    gl.disable(gl.BLEND);
    const u = this.bakeProg!.u;
    gl.useProgram(this.bakeProg!.p);
    gl.uniformMatrix4fv(u.uInvVP, false, this.cam.inv);
    gl.uniform2f(u.uRes, w, h);
    gl.uniform3fv(u.uDeskLight, s.light);
    gl.uniform3fv(u.uDeskMid, s.mid);
    gl.uniform3fv(u.uDeskDark, s.dark);
    gl.bindVertexArray(this.M!.fsTri);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }
}
