import { describe, expect, it } from "vitest";
import { withoutPage } from "./usePageNotes";

describe("withoutPage", () => {
  it("drops a page that loaded after failing", () => {
    expect([...withoutPage(new Set([2, 5]), 5)]).toEqual([2]);
  });

  it("returns the same set when the page never failed", () => {
    const failed = new Set([2]);
    expect(withoutPage(failed, 7)).toBe(failed);
  });

  it("does not change the set it was given", () => {
    const failed = new Set([3]);
    withoutPage(failed, 3);
    expect([...failed]).toEqual([3]);
  });
});
