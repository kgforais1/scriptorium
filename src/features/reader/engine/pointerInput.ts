// Event glue for pointer input on the canvas: hover peek, drag-to-turn, and the cancel paths.
// Every decision (hit areas, tap versus drag, how a release settles) lives in gesture.ts. The engine has no
// keyboard listener; the reader chrome owns keys. Under reduced motion there is no peek and no
// drag, and a tap on a page turns it with a cross-fade.

import {
  cornerAt, dragProgress, gripAt, hoverOnPage, onPageAt, pastSlop, peekStays, pickLocal, settleFor, tapDirection, trackDrag,
  type Drag, type Tap,
} from "./gesture";
import type { Book } from "./geometry";
import type { Mat4 } from "./math";
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
  private tap: Tap | null = null;
  private off: (() => void)[] = [];

  /** Call `attach` to start listening. */
  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly book: Book,
    private readonly machine: TurnMachine,
    private readonly view: PointerView,
    private reducedMotion: boolean,
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

  /** Switches between curl-and-drag and tap-to-fade input. A gesture already held finishes as it began. */
  setReducedMotion(reduced: boolean): void {
    this.reducedMotion = reduced;
  }

  private readonly onMove = (e: PointerEvent) => {
    const l = this.local(e);
    // A drag already held keeps going until release, whatever the setting is now.
    if (this.drag) {
      this.dragMove(e, l, this.drag);
    } else if (this.reducedMotion) {
      this.canvas.style.cursor = onPageAt(this.book, l, 0.06) ? "pointer" : "default";
    } else if (e.pointerType === "mouse") {
      // Hover peek is a mouse-only polish; touch and pen get the same turn by dragging.
      this.hoverMove(l);
    }
  };

  private dragMove(e: PointerEvent, l: [number, number], d: Drag) {
    const T = this.machine.turn;
    if (!T || e.pointerId !== d.id) return;
    if (pastSlop(d.sx, d.sy, e.clientX, e.clientY)) d.moved = true;
    if (!d.moved) return;
    if (T.kind === "peek") T.lift = T.pk;
    if (T.kind !== "drag") {
      T.kind = "drag";
      this.machine.host.changed();
    }
    T.anim = null;
    T.cancel = 0;
    trackDrag(d, l[0], performance.now());
    T.t = dragProgress(T.dir, d.x0, l[0]);
  }

  private hoverMove(l: [number, number]) {
    const m = this.machine, c = cornerAt(this.book, l);
    this.canvas.style.cursor = c || hoverOnPage(this.book, l) ? "grab" : "default";
    if (c && !m.turn) {
      const T = m.beginTurn(c.side > 0 ? 1 : -1, "peek", c.cv);
      if (T) T.pkT = 1;
    } else if (m.turn?.kind === "peek") {
      m.turn.pkT = peekStays(m.queued, m.turn.dir, c, m.jumpPending) ? 1 : 0;
    }
  }

  private readonly onLeave = () => {
    const T = this.machine.turn;
    if (T?.kind === "peek") T.pkT = 0;
  };

  private readonly onDown = (e: PointerEvent) => {
    if (e.button !== 0 || this.drag || this.tap) return;
    const l = this.local(e);
    if (!onPageAt(this.book, l, 0.06)) return;
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
    const m = this.machine, grip = gripAt(this.book, l[1]);
    if (m.turn && !(m.turn.kind === "peek" && m.turn.dir === dir)) return;
    const T = m.turn || m.beginTurn(dir, "drag", grip.vn);
    if (!T) return;
    if (T.kind === "peek") T.A0 = 1.3;
    else {
      T.cv = grip.vn;
      T.A0 = grip.A0;
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
    const s = settleFor(T, d, e.type, performance.now());
    this.machine.animateTo(s.target, s.dur, s.ease);
  };

  private releaseTap(e: PointerEvent) {
    const t = this.tap;
    if (t?.id !== e.pointerId) return;
    this.tap = null;
    const dir = tapDirection(t, e.type, e.clientX, e.clientY);
    if (dir) this.machine.go(dir);
  }
}
