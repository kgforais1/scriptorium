import { describe, expect, it } from "vitest";
import { stepPage } from "./paging";

describe("stepPage", () => {
  it("moves back and forward", () => {
    expect(stepPage(3, 10, "ArrowLeft")).toBe(2);
    expect(stepPage(3, 10, "ArrowRight")).toBe(4);
  });

  it("does nothing at the ends", () => {
    expect(stepPage(0, 10, "ArrowLeft")).toBe(0);
    expect(stepPage(9, 10, "ArrowRight")).toBe(9);
  });

  it("stays on page 0 for an empty or one-page book", () => {
    expect(stepPage(0, 0, "ArrowRight")).toBe(0);
    expect(stepPage(0, 1, "ArrowRight")).toBe(0);
  });

  it("returns the current page for other keys", () => {
    expect(stepPage(3, 10, "Enter")).toBe(3);
    expect(stepPage(3, 10, "ArrowUp")).toBe(3);
  });
});
