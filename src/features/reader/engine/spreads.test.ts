import { describe, expect, it } from "vitest";
import { firstPageOfSpread, pageOfSlot, pagesOfSpread, spreadCount, spreadOfPage } from "./spreads";

describe("spreadOfPage", () => {
  it("opens page 0 alone on spread 0, then pairs pages", () => {
    expect([0, 1, 2, 3, 4].map(spreadOfPage)).toEqual([0, 1, 1, 2, 2]);
  });
});

describe("firstPageOfSpread", () => {
  it("is the first scan visible in the spread", () => {
    expect([0, 1, 2, 3].map(firstPageOfSpread)).toEqual([0, 1, 3, 5]);
  });

  it("agrees with spreadOfPage", () => {
    for (let p = 0; p < 20; p++) {
      const s = spreadOfPage(p);
      expect(pagesOfSpread(s, 20)).toContain(p);
      expect(firstPageOfSpread(s)).toBeLessThanOrEqual(p);
    }
  });
});

describe("spreadCount", () => {
  it("counts spreads for a book of n pages", () => {
    expect([0, 1, 2, 3, 4, 5].map(spreadCount)).toEqual([0, 1, 2, 2, 3, 3]);
    expect(spreadCount(202)).toBe(102);
  });
});

describe("pagesOfSpread", () => {
  it("shows one scan on the first spread", () => {
    expect(pagesOfSpread(0, 5)).toEqual([0]);
  });

  it("shows two scans on a middle spread", () => {
    expect(pagesOfSpread(1, 5)).toEqual([1, 2]);
    expect(pagesOfSpread(2, 5)).toEqual([3, 4]);
  });

  it("shows a 1-page book as one spread with the back pastedown", () => {
    expect(spreadCount(1)).toBe(1);
    expect(pagesOfSpread(0, 1)).toEqual([0]);
    expect(pagesOfSpread(1, 1)).toEqual([]);
  });

  it("shows no spread for a 0-page book", () => {
    expect(pagesOfSpread(0, 0)).toEqual([]);
  });

  it("shows one scan plus the pastedown on an even last spread", () => {
    expect(pagesOfSpread(1, 2)).toEqual([1]);
    expect(pagesOfSpread(2, 4)).toEqual([3]);
  });

  it("is empty for a spread outside the book", () => {
    expect(pagesOfSpread(-1, 5)).toEqual([]);
    expect(pagesOfSpread(3, 5)).toEqual([]);
    expect(pagesOfSpread(1.5, 5)).toEqual([]);
  });
});

describe("pageOfSlot", () => {
  it("maps slots to scans and the pastedowns to null", () => {
    expect(pageOfSlot(0, 3)).toBeNull();
    expect(pageOfSlot(1, 3)).toBe(0);
    expect(pageOfSlot(3, 3)).toBe(2);
    expect(pageOfSlot(4, 3)).toBeNull();
  });
});
