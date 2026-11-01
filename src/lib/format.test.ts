import { describe, expect, it } from "vitest";
import { formatMegabytes, progressLabel } from "./format";

describe("formatMegabytes", () => {
  it("formats with one decimal", () => {
    expect(formatMegabytes(0)).toBe("0.0 MB");
    expect(formatMegabytes(1_000_000)).toBe("1.0 MB");
    expect(formatMegabytes(12_500_000)).toBe("12.5 MB");
  });
  it("treats bad input as zero", () => {
    expect(formatMegabytes(-5)).toBe("0.0 MB");
    expect(formatMegabytes(Number.NaN)).toBe("0.0 MB");
    expect(formatMegabytes(Number.POSITIVE_INFINITY)).toBe("0.0 MB");
  });
});

describe("progressLabel", () => {
  it("shows done over total", () => {
    expect(progressLabel(12, 40)).toBe("12 / 40 pages");
  });
});
