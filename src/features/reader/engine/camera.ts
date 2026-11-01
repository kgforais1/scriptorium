// Camera for the book scene: frames the open book inside the canvas with room for the chrome.
// Pure matrix math, so it is unit tested.

import type { Book } from "./geometry";
import { PI, invert, lookAt, multiply, perspective, type Mat4, type Vec3 } from "./math";

/** CSS pixels kept clear around the book: the header text, the bottom bar, and the sides. */
export const INSETS = { top: 46, bot: 82, side: 28 };

/** View-projection, its inverse, the eye position, and the pixels-per-world-unit scale. */
export type Camera = { VP: Mat4; inv: Mat4; eye: Vec3; k: number };

/** Fits the open book (both pages plus the board margin) inside the canvas minus the insets. */
export function computeCamera(cssW: number, cssH: number, b: Book, fovDegrees: number): Camera {
  const fov = (fovDegrees * PI) / 180;
  const availW = cssW - 2 * INSETS.side;
  const availH = cssH - INSETS.top - INSETS.bot;
  const hx = b.W + b.MRG + 0.027;
  const hy = b.H / 2 + b.MRG + 0.017;
  const k = Math.max(1e-3, Math.min(availW / (2 * hx), availH / (2 * hy)));
  const D = cssH / (2 * k * Math.tan(fov / 2));
  const sy = INSETS.top + availH / 2;
  const tgt: Vec3 = [0, (sy - cssH / 2) / k, 0];
  const eye: Vec3 = [tgt[0], tgt[1], D];
  const VP = multiply(perspective(fov, cssW / cssH, 0.3, D + 10), lookAt(eye, tgt, [0, 1, 0]));
  return { VP, inv: invert(VP), eye, k };
}
