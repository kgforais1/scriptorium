import { describe, expect, it } from "vitest";
import { clampPage } from "./clamp";

describe("clampPage", () => {
  it("keeps an in-range page", () => {
    expect(clampPage(3, 10)).toBe(3);
  });

  it("clamps a negative page to 0", () => {
    expect(clampPage(-4, 10)).toBe(0);
  });

  it("clamps a page past the end to the last page", () => {
    expect(clampPage(10, 10)).toBe(9);
    expect(clampPage(99, 10)).toBe(9);
  });

  it("returns 0 for an empty book", () => {
    expect(clampPage(5, 0)).toBe(0);
    expect(clampPage(-1, 0)).toBe(0);
  });

  it("returns 0 for a one-page book", () => {
    expect(clampPage(7, 1)).toBe(0);
  });
});
