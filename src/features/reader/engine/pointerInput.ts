// Pointer input on the canvas: hover peek, drag-to-turn, and the cancel paths. The engine has no
// keyboard listener; the reader chrome owns keys. Under reduced motion there is no peek and no
// drag, and a tap on a page turns it with a cross-fade.

import { cornerAt, dragProgress, pickLocal, turnCompletes, type Drag } from "./gesture";
import type { Book } from "./geometry";
import { clamp, easeInOut, easeOut, lerp, type Mat4 } from "./math";
import type { TurnMachine } from "./turnMachine";

/** What the input needs to know about the view. */
export interface PointerView {
  /** Inverse view-projection of the current camera. */
  inverse(): Mat4;
  /** The canvas size in CSS pixels. */
  size(): [number, number];
}

/** Turns pointer events on a canvas into turn-machine calls. */
export class PointerInput {
  private drag: Drag | null = null;
  private tap: { id: number; sx: number; sy: number; dir: 1 | -1 } | null = null;
  private off: (() => void)[] = [];

  /** Call `attach` to start listening. */
  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly book: Book,
    private readonly machine: TurnMachine,
    private readonly view: PointerView,
    private readonly reducedMotion: boolean,
  ) {}

  /** Starts listening on the canvas. */
  attach(): void {
    const on = <K extends keyof HTMLElementEventMap>(t: K, f: (e: HTMLElementEventMap[K]) => void) => {
      this.canvas.addEventListener(t, f);
      this.off.push(() => this.canvas.removeEventListener(t, f));
    };
    on("pointermove", this.onMove);
    on("pointerleave", this.onLeave);
    on("pointerdown", this.onDown);
    on("pointerup", this.onRelease);
    on("pointercancel", this.onRelease);
    on("lostpointercapture", this.onRelease);
  }

  /** Stops listening and lets go of a held page. */
  detach(): void {
    for (const off of this.off) off();
    this.off = [];
    this.drag = null;
    this.tap = null;
    this.machine.dragging = false;
  }

  private local(e: PointerEvent): [number, number] {
    const r = this.canvas.getBoundingClientRect();
    const [w, h] = this.view.size();
    return pickLocal(this.view.inverse(), w, h, e.clientX - r.left, e.clientY - r.top);
  }

  // Where a press can grab: on a page, off the gutter.
  private onPage(l: [number, number], slack: number): boolean {
    return Math.abs(l[1]) <= this.book.H / 2 + slack && Math.abs(l[0]) <= 1.08 && Math.abs(l[0]) >= 0.03;
  }

  private readonly onMove = (e: PointerEvent) => {
    const l = this.local(e);
    if (this.reducedMotion) {
      this.canvas.style.cursor = this.onPage(l, 0.06) ? "pointer" : "default";
    } else if (this.drag) {
      this.dragMove(e, l, this.drag);
    } else if (e.pointerType === "mouse") {
      // Hover peek is a mouse-only polish; touch and pen get the same turn by dragging.
      this.hoverMove(l);
    }
  };

  private dragMove(e: PointerEvent, l: [number, number], d: Drag) {
    const T = this.machine.turn;
    if (!T || e.pointerId !== d.id) return;
    if (Math.abs(e.clientX - d.sx) > 4 || Math.abs(e.clientY - d.sy) > 4) d.moved = true;
    if (!d.moved) return;
    if (T.kind === "peek") T.lift = T.pk;
    if (T.kind !== "drag") {
      T.kind = "drag";
      this.machine.host.changed();
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
    const m = this.machine, c = cornerAt(this.book, l), h2 = this.book.H / 2;
    const onPage = Math.abs(l[1]) < h2 && Math.abs(l[0]) < 1.05 && Math.abs(l[0]) > 0.04;
    this.canvas.style.cursor = c || onPage ? "grab" : "default";
    if (c && !m.turn) {
      const T = m.beginTurn(c.side > 0 ? 1 : -1, "peek", c.cv);
      if (T) T.pkT = 1;
    } else if (m.turn?.kind === "peek") {
      // Once navigation has queued a step, the peek must settle; hovering cannot re-arm it.
      m.turn.pkT = m.queued === 0 && m.turn.dir === c?.side ? 1 : 0;
    }
  }

  private readonly onLeave = () => {
    const T = this.machine.turn;
    if (T?.kind === "peek") T.pkT = 0;
  };

  private readonly onDown = (e: PointerEvent) => {
    if (e.button !== 0 || this.drag || this.tap) return;
    const l = this.local(e);
    if (!this.onPage(l, 0.06)) return;
    const dir = l[0] > 0 ? 1 : -1;
    if (this.reducedMotion) {
      this.tap = { id: e.pointerId, sx: e.clientX, sy: e.clientY, dir };
      this.canvas.setPointerCapture(e.pointerId);
      e.preventDefault();
      return;
    }
    this.grab(e, l, dir);
  };

  private grab(e: PointerEvent, l: [number, number], dir: 1 | -1) {
    const m = this.machine, h2 = this.book.H / 2;
    if (m.turn && !(m.turn.kind === "peek" && m.turn.dir === dir)) return;
    const vn = clamp((h2 - l[1]) / this.book.H, 0, 1);
    const T = m.turn || m.beginTurn(dir, "drag", vn);
    if (!T) return;
    if (T.kind === "peek") T.A0 = 1.3;
    else {
      T.cv = vn;
      T.A0 = Math.abs(vn - 0.5) > 0.32 ? 1.3 : 0.45;
    }
    this.drag = { id: e.pointerId, sx: e.clientX, sy: e.clientY, x0: l[0], lx: l[0], lt: performance.now(), vx: 0, moved: false };
    m.dragging = true;
    this.canvas.setPointerCapture(e.pointerId);
    this.canvas.style.cursor = "grabbing";
    e.preventDefault();
  }

  // Release, pointercancel, and lost capture all settle the page, so the book never sticks mid-turn.
  private readonly onRelease = (e: PointerEvent) => {
    if (this.tap) return this.releaseTap(e);
    const d = this.drag;
    if (d?.id !== e.pointerId) return;
    this.drag = null;
    this.machine.dragging = false;
    this.canvas.style.cursor = "grab";
    const T = this.machine.turn;
    if (!T) return;
    const up = e.type === "pointerup";
    if (!d.moved) {
      if (up) this.machine.animateTo(T.dir > 0 ? 1 : 0, 1.05, easeInOut);
      else this.machine.animateTo(T.dir > 0 ? 0 : 1, 0.3, easeOut);
      return;
    }
    const target = (T.dir > 0) === turnCompletes(T, d, up) ? 1 : 0;
    this.machine.animateTo(target, 0.3 + 0.55 * Math.abs(target - T.t), easeOut);
  };

  private releaseTap(e: PointerEvent) {
    const t = this.tap;
    if (t?.id !== e.pointerId) return;
    this.tap = null;
    const still = Math.abs(e.clientX - t.sx) <= 4 && Math.abs(e.clientY - t.sy) <= 4;
    if (e.type === "pointerup" && still) this.machine.go(t.dir);
  }
}
