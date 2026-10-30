import { describe, expect, it } from "vitest";
import { TEX_HEIGHT, textureHeight } from "./texSize";

describe("textureHeight", () => {
  it("uses the fixed height when the GPU allows it", () => {
    expect(textureHeight(1.4, 16384)).toBe(TEX_HEIGHT);
  });

  it("is limited by the GPU maximum", () => {
    expect(textureHeight(1.4, 1024)).toBe(1024);
  });

  it("lowers the height for a wide book so the width fits", () => {
    // A landscape page box (aspect 0.5) is twice as wide as tall.
    const h = textureHeight(0.5, 2048);
    expect(h).toBe(1024);
    expect(h / 0.5).toBeLessThanOrEqual(2048);
  });

  it("never returns less than 1", () => {
    expect(textureHeight(0.0001, 1)).toBe(1);
  });
});
