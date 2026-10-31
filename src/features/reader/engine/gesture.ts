// Pure gesture rules for the page-turn engine: hit areas, drag progress, and release outcome.
// Coordinates are book-local world units (page width 1, gutter at x = 0).

import { clamp, easeInOut, easeOut, lerp, transform, type Mat4 } from "./math";
import type { Book, Turn } from "./geometry";

export type Drag = { id: number; sx: number; sy: number; x0: number; lx: number; lt: number; vx: number; moved: boolean };

/** Casts a ray from a CSS pixel through the inverse view-projection and returns where it meets the plane z = zp, in book-local x and y. */
export function pickLocal(inv: Mat4, cssW: number, cssH: number, mx: number, my: number, zp = 0.07): [number, number] {
  const nx = (mx / cssW) * 2 - 1, ny = 1 - (my / cssH) * 2;
  let a = transform(inv, [nx, ny, -1, 1]), b = transform(inv, [nx, ny, 1, 1]);
  a = a.map((v) => v / a[3]) as typeof a;
  b = b.map((v) => v / b[3]) as typeof b;
  const t = (zp - a[2]) / (b[2] - a[2]);
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
}

/** The grab zone around each outer page corner, or null. cv is 0 for a top corner and 1 for a bottom one. */
export function cornerAt(book: Book, l: [number, number]) {
  const ax = Math.abs(l[0]), ay = Math.abs(l[1]), h2 = book.H / 2;
  // The zone reaches 0.2 in from the head or tail, but never past the middle of a short page.
  if (ax > 0.8 && ax < 1.08 && ay > Math.max(h2 - 0.2, h2 / 2) && ay < h2 + 0.06) return { side: Math.sign(l[0]), cv: l[1] < 0 ? 1 : 0 };
  return null;
}

/** Turn progress t (0 = leaf on the right, 1 = on the left) for a drag that started at x0 and is now at x. */
export function dragProgress(dir: 1 | -1, x0: number, x: number): number {
  if (dir > 0) return clamp((x0 - x) / (x0 + 0.95), 0, 1);
  return 1 - clamp((x - x0) / (0.95 - x0), 0, 1);
}

/** Whether a released drag completes the turn. It does past the midpoint or on a flick toward the far side. A cancelled drag ignores velocity and settles to whichever side is nearer. */
export function turnCompletes(T: Pick<Turn, "dir" | "t">, d: Drag, released: boolean): boolean {
  const fwd = T.dir > 0;
  if (!released) return fwd ? T.t > 0.5 : T.t < 0.5;
  if (fwd) return (T.t > 0.45 || d.vx < -1.2) && d.vx < 1;
  return (T.t < 0.55 || d.vx > 1.2) && d.vx > -1;
}

/** Pointer travel, in CSS pixels, below which a press still counts as a tap. */
export const TAP_SLOP = 4;

/** Whether the pointer has left the slop circle around where it went down. */
export function pastSlop(sx: number, sy: number, x: number, y: number): boolean {
  return Math.abs(x - sx) > TAP_SLOP || Math.abs(y - sy) > TAP_SLOP;
}

/**
 * Whether a press at book-local l can grab a page: on the page and off the gutter. `slack` lets a
 * press just past the head or tail still count.
 */
export function onPageAt(book: Book, l: [number, number], slack: number): boolean {
  return Math.abs(l[1]) <= book.H / 2 + slack && Math.abs(l[0]) <= 1.08 && Math.abs(l[0]) >= 0.03;
}

/** Whether a hovering mouse is over a page, for the grab cursor. */
export function hoverOnPage(book: Book, l: [number, number]): boolean {
  return Math.abs(l[1]) < book.H / 2 && Math.abs(l[0]) < 1.05 && Math.abs(l[0]) > 0.04;
}

/** Where a grab holds the page: vn is 0 at the head and 1 at the tail, A0 is how much corner lift it implies. */
export function gripAt(book: Book, y: number): { vn: number; A0: number } {
  const vn = clamp((book.H / 2 - y) / book.H, 0, 1);
  return { vn, A0: Math.abs(vn - 0.5) > 0.32 ? 1.3 : 0.45 };
}

/**
 * Whether a hovering peek should stay open. Once navigation has queued a step the peek must
 * settle; hovering cannot re-arm it. Otherwise it stays while the mouse is on its own corner.
 */
export function peekStays(queued: number, peekDir: number, corner: { side: number } | null): boolean {
  return queued === 0 && peekDir === corner?.side;
}

/** Updates a drag's smoothed horizontal velocity (book units per second) and position. */
export function trackDrag(d: Drag, x: number, now: number): void {
  const dt = Math.max(1, now - d.lt);
  d.vx = lerp(d.vx, ((x - d.lx) / dt) * 1000, 0.5);
  d.lx = x;
  d.lt = now;
}

/** How a leaf settles: its end state (0 right, 1 left), the duration in seconds, and the easing. */
export type Settle = { target: 0 | 1; dur: number; ease: (x: number) => number };

/**
 * What a released, cancelled, or capture-lost drag does. A pointerup that never moved turns the
 * page like a click. Any other end of an unmoved drag (pointercancel, lostpointercapture) puts
 * the page back. A moved drag settles by `turnCompletes`: a release may flick, the others go to
 * the nearer side.
 */
export function settleFor(T: Pick<Turn, "dir" | "t">, d: Drag, eventType: string): Settle {
  const released = eventType === "pointerup", fwd = T.dir > 0;
  if (!d.moved) {
    return released
      ? { target: fwd ? 1 : 0, dur: 1.05, ease: easeInOut }
      : { target: fwd ? 0 : 1, dur: 0.3, ease: easeOut };
  }
  const target = fwd === turnCompletes(T, d, released) ? 1 : 0;
  return { target, dur: 0.3 + 0.55 * Math.abs(target - T.t), ease: easeOut };
}

/** A press held under reduced motion, waiting to become a tap. */
export type Tap = { id: number; sx: number; sy: number; dir: 1 | -1 };

/** The direction a tap turns, or null when it ended on a cancel, a lost capture, or after moving. */
export function tapDirection(t: Tap, eventType: string, x: number, y: number): 1 | -1 | null {
  return eventType === "pointerup" && !pastSlop(t.sx, t.sy, x, y) ? t.dir : null;
}
