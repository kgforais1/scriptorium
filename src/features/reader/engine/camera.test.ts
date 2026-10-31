import { describe, expect, it } from "vitest";
import { computeCamera, INSETS } from "./camera";
import { makeBook } from "./geometry";
import { pickLocal } from "./gesture";
import { transform } from "./math";

describe("computeCamera", () => {
  const book = makeBook(1.4, 10);

  it("looks at the book from the +z side", () => {
    const cam = computeCamera(1200, 800, book, 18);
    expect(cam.eye[2]).toBeGreaterThan(1);
    expect(cam.k).toBeGreaterThan(0);
  });

  it("scales with the tighter of the width and height limits", () => {
    const wide = computeCamera(3000, 600, book, 18);
    const narrow = computeCamera(600, 3000, book, 18);
    expect(narrow.k).toBeCloseTo((600 - 2 * INSETS.side) / (2 * (book.W + book.MRG + 0.027)));
    expect(wide.k).toBeCloseTo((600 - INSETS.top - INSETS.bot) / (2 * (book.H / 2 + book.MRG + 0.017)));
  });

  it("stays finite for a canvas smaller than the insets", () => {
    const cam = computeCamera(10, 10, book, 18);
    expect(Number.isFinite(cam.k)).toBe(true);
    expect(cam.k).toBeGreaterThan(0);
  });

  it("round-trips the gutter point through project and pick", () => {
    const cam = computeCamera(1200, 800, book, 18);
    for (const [x, y] of [[0.5, 0.2], [-0.8, -0.3], [0, 0]]) {
      const c = transform(cam.VP, [x, y, 0.07, 1]);
      const px = ((c[0] / c[3]) * 0.5 + 0.5) * 1200, py = (1 - ((c[1] / c[3]) * 0.5 + 0.5)) * 800;
      const [lx, ly] = pickLocal(cam.inv, 1200, 800, px, py);
      expect(lx).toBeCloseTo(x, 3);
      expect(ly).toBeCloseTo(y, 3);
    }
  });

  it("centres the book vertically between the top and bottom insets", () => {
    for (const [w, h] of [[1200, 800], [900, 900], [1600, 700]]) {
      const cam = computeCamera(w, h, book, 18);
      const c = transform(cam.VP, [0, 0, 0, 1]);
      const py = (1 - ((c[1] / c[3]) * 0.5 + 0.5)) * h;
      expect(Math.abs(py - (INSETS.top + h - INSETS.bot) / 2)).toBeLessThan(1);
    }
  });
});
