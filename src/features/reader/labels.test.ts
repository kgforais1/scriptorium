import { describe, expect, it } from "vitest";
import { canvasLabel, failedPagesNote, pageIndicator, pageRange, pageToSave } from "./labels";

describe("pageIndicator", () => {
  it("reads page 1 of N on spread 0", () => {
    expect(pageIndicator(0, 202)).toBe("page 1 of 202");
  });

  it("reads a pair of pages with an en dash", () => {
    expect(pageIndicator(2, 202)).toBe("pages 4–5 of 202");
    expect(pageIndicator(2, 6)).toBe("pages 4–5 of 6");
  });

  it("says page for a last spread with one scan", () => {
    // 5 scans: spreads are [0], [1,2], [3,4]; 4 scans: [0], [1,2], [3]
    expect(pageIndicator(2, 4)).toBe("page 4 of 4");
    expect(pageIndicator(2, 5)).toBe("pages 4–5 of 5");
  });

  it("handles an empty book and an out-of-range spread", () => {
    expect(pageIndicator(0, 0)).toBe("no pages");
    expect(pageIndicator(9, 4)).toBe("no pages");
  });

  it("reads page 1 of 1 for a one-page book", () => {
    expect(pageIndicator(0, 1)).toBe("page 1 of 1");
  });
});

describe("pageRange", () => {
  it("gives a single number, a range, or nothing", () => {
    expect(pageRange(0, 10)).toBe("1");
    expect(pageRange(2, 10)).toBe("4–5");
    expect(pageRange(-1, 10)).toBe("");
  });
});

describe("canvasLabel", () => {
  it("announces the open pages", () => {
    expect(canvasLabel(2, 202)).toBe("Open book, pages 4–5 of 202");
    expect(canvasLabel(0, 202)).toBe("Open book, page 1 of 202");
  });
});

describe("pageToSave", () => {
  it("saves the first scan of a new spread, never the spread index", () => {
    expect(pageToSave(0, 1)).toBe(1);
    expect(pageToSave(1, 2)).toBe(3);
    expect(pageToSave(5, 0)).toBe(0);
  });

  it("saves nothing when the spread did not change", () => {
    expect(pageToSave(3, 3)).toBeNull();
  });
});

describe("failedPagesNote", () => {
  it("names one failed page of the spread", () => {
    expect(failedPagesNote(new Set([6]), 3, 202)).toBe("Page 7 could not be loaded.");
  });

  it("names both pages of a spread", () => {
    expect(failedPagesNote(new Set([5, 6]), 3, 202)).toBe("Pages 6 and 7 could not be loaded.");
  });

  it("ignores failures on other spreads, and has no note when none failed", () => {
    expect(failedPagesNote(new Set([20]), 3, 202)).toBeNull();
    expect(failedPagesNote(new Set(), 3, 202)).toBeNull();
  });

  it("keeps the note when the reader turns back to a failed page", () => {
    const failed = new Set([6]);
    expect(failedPagesNote(failed, 4, 202)).toBeNull();
    expect(failedPagesNote(failed, 3, 202)).toBe("Page 7 could not be loaded.");
  });

  it("names the lone first page", () => {
    expect(failedPagesNote(new Set([0]), 0, 202)).toBe("Page 1 could not be loaded.");
  });
});
