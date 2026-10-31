// Projected page corners for M2's text overlay and M6's hotspots. The engine writes them into
// one preallocated buffer, so publishing them costs no allocation.
//
// Layout: GEOMETRY_SLOTS slots of GEOMETRY_STRIDE floats each.
//   slot 0: the page on the left block, slot 1: the page on the right block,
//   slot 2: the visible face of the turning leaf (unused when nothing turns).
// A slot is [page, x0, y0, x1, y1, x2, y2, x3, y3]. `page` is the 0-based scan index, or -1 for
// a pastedown or an unused slot. The corners run top-left, top-right, bottom-right, bottom-left
// as the page reads. Coordinates are canvas pixels (the drawing buffer, CSS size times the
// device pixel ratio), origin top-left.

import { NU, NV, type Profile, type Book } from "./geometry";
import type { Mat4 } from "./math";

/** Slots in the buffer: left page, right page, turning leaf. */
export const GEOMETRY_SLOTS = 3;
/** Floats per slot: the page index plus four x, y pairs. */
export const GEOMETRY_STRIDE = 9;

/** Allocates the buffer once. The engine reuses it for every publish. */
export function createGeometryBuffer(): Float32Array {
  return new Float32Array(GEOMETRY_SLOTS * GEOMETRY_STRIDE);
}

/** Where to project to: the camera and the drawing-buffer size. */
export type Target = { VP: Mat4; w: number; h: number };

/** Projects a world point into canvas pixels and writes x, y at out[o], out[o + 1]. */
export function projectInto(out: Float32Array, o: number, t: Target, x: number, y: number, z: number) {
  const { VP, w, h } = t;
  const cx = VP[0] * x + VP[4] * y + VP[8] * z + VP[12];
  const cy = VP[1] * x + VP[5] * y + VP[9] * z + VP[13];
  const cw = VP[3] * x + VP[7] * y + VP[11] * z + VP[15] || 1;
  out[o] = ((cx / cw) * 0.5 + 0.5) * w;
  out[o + 1] = (1 - ((cy / cw) * 0.5 + 0.5)) * h;
}

/** Marks a slot unused. */
export function clearSlot(out: Float32Array, slot: number) {
  out.fill(0, slot * GEOMETRY_STRIDE + 1, (slot + 1) * GEOMETRY_STRIDE);
  out[slot * GEOMETRY_STRIDE] = -1;
}

/** Writes the resting page on one block. sig is -1 for the left block, +1 for the right. */
export function writeStackSlot(out: Float32Array, slot: number, page: number, prof: Profile, sig: number, b: Book, t: Target) {
  const o = slot * GEOMETRY_STRIDE;
  const zIn = prof.Z[0], zOut = prof.Z[NU - 1], xOut = sig * prof.X[NU - 1], top = b.H / 2, bot = -b.H / 2;
  // The gutter is the inner edge; "top-left" is the outer edge on the left block.
  const [xl, zl, xr, zr] = sig < 0 ? [xOut, zOut, 0, zIn] : [0, zIn, xOut, zOut];
  out[o] = page;
  projectInto(out, o + 1, t, xl, top, zl);
  projectInto(out, o + 3, t, xr, top, zr);
  projectInto(out, o + 5, t, xr, bot, zr);
  projectInto(out, o + 7, t, xl, bot, zl);
}

/**
 * Writes the turning leaf's visible face from its vertex grid. The front face reads with the
 * hinge on the left, the back face with the hinge on the right.
 */
export function writeLeafSlot(out: Float32Array, slot: number, page: number, leafV: Float32Array, back: boolean, t: Target) {
  const o = slot * GEOMETRY_STRIDE;
  const hinge = 0, edge = NU - 1, tail = NV - 1;
  const at = (j: number, i: number, k: number) => {
    const v = (j * NU + i) * 8;
    projectInto(out, o + k, t, leafV[v], leafV[v + 1], leafV[v + 2]);
  };
  const [iL, iR] = back ? [edge, hinge] : [hinge, edge];
  out[o] = page;
  at(0, iL, 1);
  at(0, iR, 3);
  at(tail, iR, 5);
  at(tail, iL, 7);
}
