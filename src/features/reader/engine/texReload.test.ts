import { describe, expect, it } from "vitest";
import { needsReload, RELOAD_RATIO, slotsToReload } from "./texReload";

describe("needsReload", () => {
  it("rebuilds a texture much smaller than the target", () => {
    expect(needsReload(2, 800)).toBe(true);
    expect(needsReload(400, 1000)).toBe(true);
  });

  it("leaves a texture alone within the ratio, so small resizes do not thrash", () => {
    expect(needsReload(800, 800)).toBe(false);
    expect(needsReload(800, 800 * RELOAD_RATIO)).toBe(false);
    expect(needsReload(800, 900)).toBe(false);
  });

  it("rebuilds just past the ratio", () => {
    expect(needsReload(800, 800 * RELOAD_RATIO + 1)).toBe(true);
  });

  it("never rebuilds for a smaller target", () => {
    expect(needsReload(2048, 500)).toBe(false);
    expect(needsReload(1, 0)).toBe(false);
  });

  it("takes the ratio as a parameter", () => {
    expect(needsReload(100, 150, 2)).toBe(false);
    expect(needsReload(100, 250, 2)).toBe(true);
  });
});

describe("slotsToReload", () => {
  const built: [number, number][] = [[1, 2], [2, 800], [3, 600], [4, 1024], [5, 1]];

  it("picks the slots built too small for the target", () => {
    expect(slotsToReload(built, 800)).toEqual([1, 3, 5]);
  });

  it("picks nothing when every texture is close to the target", () => {
    expect(slotsToReload([[1, 780], [2, 800], [3, 850]], 800)).toEqual([]);
  });

  it("picks everything built tiny after a window is maximised", () => {
    expect(slotsToReload([[1, 150], [2, 150], [3, 150]], 2048)).toEqual([1, 2, 3]);
  });

  it("picks nothing when the target shrinks", () => {
    expect(slotsToReload([[1, 800], [2, 1024]], 100)).toEqual([]);
  });

  it("handles no textures", () => {
    expect(slotsToReload([], 2048)).toEqual([]);
  });
});
