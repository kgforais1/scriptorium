import { describe, expect, it, vi } from "vitest";
import { applyKeyAction, keyAction, shouldIgnoreKey, type KeyLike } from "./keys";

const plain: KeyLike = { altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, target: {} };

describe("keyAction", () => {
  it.each([
    ["ArrowRight", "next"],
    ["PageDown", "next"],
    ["ArrowLeft", "prev"],
    ["PageUp", "prev"],
    ["Home", "first"],
    ["End", "last"],
  ])("%s -> %s", (key, action) => {
    expect(keyAction(key)).toBe(action);
  });

  it("ignores other keys, including object prototype names", () => {
    expect(keyAction("a")).toBeNull();
    expect(keyAction("Enter")).toBeNull();
    expect(keyAction("toString")).toBeNull();
  });
});

describe("shouldIgnoreKey", () => {
  it("lets a bare key through", () => {
    expect(shouldIgnoreKey(plain, false)).toBe(false);
  });

  it.each(["altKey", "ctrlKey", "metaKey", "shiftKey"] as const)("ignores %s", (flag) => {
    expect(shouldIgnoreKey({ ...plain, [flag]: true }, false)).toBe(true);
  });

  it("ignores keys typed into a field, including the scrubber", () => {
    expect(shouldIgnoreKey({ ...plain, target: { tagName: "INPUT" } }, false)).toBe(true);
    expect(shouldIgnoreKey({ ...plain, target: { isContentEditable: true } }, false)).toBe(true);
  });

  it("ignores keys while a dialog is open", () => {
    expect(shouldIgnoreKey(plain, true)).toBe(true);
  });

  it("does not ignore a button or the canvas", () => {
    expect(shouldIgnoreKey({ ...plain, target: { tagName: "BUTTON" } }, false)).toBe(false);
    expect(shouldIgnoreKey({ ...plain, target: { tagName: "CANVAS" } }, false)).toBe(false);
  });
});

describe("applyKeyAction", () => {
  const make = () => ({ next: vi.fn(), prev: vi.fn(), goTo: vi.fn() });

  it("next and prev go through the queueing methods", () => {
    const p = make();
    applyKeyAction("next", p, 10);
    applyKeyAction("prev", p, 10);
    expect(p.next).toHaveBeenCalledTimes(1);
    expect(p.prev).toHaveBeenCalledTimes(1);
    expect(p.goTo).not.toHaveBeenCalled();
  });

  it("first and last jump to the ends", () => {
    const p = make();
    applyKeyAction("first", p, 10);
    applyKeyAction("last", p, 10);
    expect(p.goTo.mock.calls).toEqual([[0], [9]]);
  });

  it("last on an empty book goes to 0", () => {
    const p = make();
    applyKeyAction("last", p, 0);
    expect(p.goTo).toHaveBeenCalledWith(0);
  });
});
