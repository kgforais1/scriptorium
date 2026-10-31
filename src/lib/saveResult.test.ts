import { describe, expect, it } from "vitest";
import { saveResultGate } from "./saveResult";

describe("saveResultGate", () => {
  it("ignores a result older than one already shown", () => {
    const gate = saveResultGate();
    const first = gate.next();
    const second = gate.next();
    expect(gate.accept(second)).toBe(true);
    expect(gate.accept(first)).toBe(false);
  });

  it("lets a newer result through after an older one", () => {
    const gate = saveResultGate();
    const first = gate.next();
    const second = gate.next();
    expect(gate.accept(first)).toBe(true);
    expect(gate.accept(second)).toBe(true);
  });

  it("settle makes saves started so far stale but not later ones", () => {
    const gate = saveResultGate();
    const inFlight = gate.next();
    gate.settle();
    expect(gate.accept(inFlight)).toBe(false);
    expect(gate.accept(gate.next())).toBe(true);
  });
});
