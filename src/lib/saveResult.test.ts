import { describe, expect, it } from "vitest";
import { saveResultGate } from "./saveResult";

describe("saveResultGate", () => {
  it("ignores a result older than one already shown", () => {
    const gate = saveResultGate();
    expect(gate.accept(2)).toBe(true);
    expect(gate.accept(1)).toBe(false);
  });

  it("lets a newer result through after an older one", () => {
    const gate = saveResultGate();
    expect(gate.accept(1)).toBe(true);
    expect(gate.accept(2)).toBe(true);
  });

  it("settle makes saves up to the written one stale but not later ones", () => {
    const gate = saveResultGate();
    gate.settle(5, 5);
    expect(gate.accept(5)).toBe(false);
    expect(gate.accept(4)).toBe(false);
    expect(gate.accept(6)).toBe(true);
  });

  it("settle clears the note only when the write reaches the latest save issued", () => {
    const gate = saveResultGate();
    expect(gate.settle(5, 5)).toBe(true);
    expect(gate.settle(6, 5)).toBe(true);
    expect(gate.settle(4, 5)).toBe(false);
  });

  it("a write older than the latest save keeps the newer save's error", () => {
    const gate = saveResultGate();
    // Saves 4 and 5 are in flight. A background write of 4 lands first.
    expect(gate.settle(4, 5)).toBe(false);
    expect(gate.accept(5)).toBe(true);
    expect(gate.accept(4)).toBe(false);
  });

  it("settle never moves the marker back", () => {
    const gate = saveResultGate();
    gate.accept(9);
    gate.settle(3, 9);
    expect(gate.accept(8)).toBe(false);
    expect(gate.accept(9)).toBe(true);
  });
});
