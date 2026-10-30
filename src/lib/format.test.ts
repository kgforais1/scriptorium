import { describe, expect, it } from "vitest";
import { formatMegabytes, pageAlt, pageLabel, progressLabel } from "./format";

describe("formatMegabytes", () => {
  it("formats with one decimal", () => {
    expect(formatMegabytes(0)).toBe("0.0 MB");
    expect(formatMegabytes(1024 * 1024)).toBe("1.0 MB");
    expect(formatMegabytes(13_107_200)).toBe("12.5 MB");
  });
  it("treats bad input as zero", () => {
    expect(formatMegabytes(-5)).toBe("0.0 MB");
    expect(formatMegabytes(Number.NaN)).toBe("0.0 MB");
    expect(formatMegabytes(Number.POSITIVE_INFINITY)).toBe("0.0 MB");
  });
});

describe("pageLabel", () => {
  it("is one-based", () => {
    expect(pageLabel(0, 40)).toBe("1 / 40");
    expect(pageLabel(39, 40)).toBe("40 / 40");
  });
  it("handles an empty book", () => {
    expect(pageLabel(0, 0)).toBe("0 / 0");
  });
});

describe("pageAlt", () => {
  it("names the page", () => {
    expect(pageAlt(2, 40)).toBe("Page 3 of 40");
  });
});

describe("progressLabel", () => {
  it("shows done over total", () => {
    expect(progressLabel(12, 40)).toBe("12 / 40 pages");
  });
});
