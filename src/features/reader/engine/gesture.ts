// Pure gesture rules for the page-turn engine: hit areas, drag progress, and release outcome.
// Coordinates are book-local world units (page width 1, gutter at x = 0).

import { clamp, transform, type Mat4 } from "./math";
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
export function turnCompletes(T: Turn, d: Drag, released: boolean): boolean {
  const fwd = T.dir > 0;
  if (!released) return fwd ? T.t > 0.5 : T.t < 0.5;
  if (fwd) return (T.t > 0.45 || d.vx < -1.2) && d.vx < 1;
  return (T.t < 0.55 || d.vx > 1.2) && d.vx > -1;
}
