import { describe, expect, it } from "vitest";
import { fitRect } from "./fit";

describe("fitRect", () => {
  it("fills the box when the scan has the box aspect", () => {
    expect(fitRect(100, 150, 1.5)).toEqual({ x: 0, y: 0, w: 1, h: 1 });
  });

  it("pillarboxes a scan narrower than the box", () => {
    const r = fitRect(100, 300, 1.5);
    expect(r).toEqual({ x: 0.25, y: 0, w: 0.5, h: 1 });
  });

  it("letterboxes a scan wider than the box", () => {
    const r = fitRect(300, 100, 1.5);
    expect(r.x).toBe(0);
    expect(r.w).toBe(1);
    expect(r.h).toBeCloseTo(1 / 3 / 1.5);
    expect(r.y).toBeCloseTo((1 - r.h) / 2);
  });

  it("never stretches: the fitted rect keeps the scan aspect", () => {
    for (const [w, h] of [[100, 144], [200, 100], [100, 100], [640, 480]]) {
      const box = 1.4, r = fitRect(w, h, box);
      expect((r.h * box) / r.w).toBeCloseTo(h / w);
      expect(r.x).toBeGreaterThanOrEqual(0);
      expect(r.y).toBeGreaterThanOrEqual(0);
      expect(r.x + r.w).toBeLessThanOrEqual(1 + 1e-9);
      expect(r.y + r.h).toBeLessThanOrEqual(1 + 1e-9);
    }
  });
});
