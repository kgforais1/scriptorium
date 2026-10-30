import { describe, expect, it } from "vitest";
import { isTypingTarget } from "./dom";

describe("isTypingTarget", () => {
  it("is true for text controls", () => {
    expect(isTypingTarget({ tagName: "INPUT" })).toBe(true);
    expect(isTypingTarget({ tagName: "TEXTAREA" })).toBe(true);
    expect(isTypingTarget({ tagName: "SELECT" })).toBe(true);
  });
  it("is true for editable elements", () => {
    expect(isTypingTarget({ tagName: "DIV", isContentEditable: true })).toBe(true);
  });
  it("is false for buttons, the body, and non-elements", () => {
    expect(isTypingTarget({ tagName: "BUTTON" })).toBe(false);
    expect(isTypingTarget({ tagName: "BODY", isContentEditable: false })).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
    expect(isTypingTarget("input")).toBe(false);
    expect(isTypingTarget({})).toBe(false);
  });
});
