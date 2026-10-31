import { describe, expect, it } from "vitest";
import { TEX_BUDGET, TEX_HEIGHT_MAX, textureHeight } from "./texSize";

const MB = 1024 * 1024;

describe("TEX_BUDGET", () => {
  it("is 256 MB", () => {
    expect(TEX_BUDGET).toBe(256 * MB);
  });
});

describe("textureHeight", () => {
  // boxAspect is height over width. devicePixelHeight is the drawing buffer's height.
  const cases: { name: string; boxAspect: number; maxTex: number; devicePixelHeight: number; expected: number }[] = [
    { name: "portrait page on a large display is capped at 2048", boxAspect: 1.4, maxTex: 16384, devicePixelHeight: 2880, expected: 2048 },
    { name: "portrait page on a 1x display follows the display", boxAspect: 1.4, maxTex: 16384, devicePixelHeight: 900, expected: 900 },
    {
      name: "landscape page is held down by the budget: floor(sqrt(256 MB * 0.5 / 64))",
      boxAspect: 0.5, maxTex: 16384, devicePixelHeight: 2880, expected: Math.floor(Math.sqrt((256 * MB * 0.5) / 64)),
    },
    { name: "square page fits the budget at 2048 (sqrt(256 MB / 64) = 2048)", boxAspect: 1, maxTex: 16384, devicePixelHeight: 2880, expected: 2048 },
    { name: "very wide page is held down further by the budget", boxAspect: 0.25, maxTex: 16384, devicePixelHeight: 2880, expected: 1024 },
    { name: "small GPU limits the height to MAX_TEXTURE_SIZE", boxAspect: 1.4, maxTex: 1024, devicePixelHeight: 2880, expected: 1024 },
    {
      name: "small GPU and a landscape page limit the height so the width fits",
      boxAspect: 0.5, maxTex: 2048, devicePixelHeight: 2880, expected: 1024,
    },
    { name: "high-DPR display is still capped at 2048", boxAspect: 1.4, maxTex: 16384, devicePixelHeight: 3 * 1300, expected: 2048 },
    { name: "high-DPR display on a small window follows the buffer", boxAspect: 1.4, maxTex: 16384, devicePixelHeight: 2 * 500, expected: 1000 },
    { name: "a tiny buffer gives a tiny texture, never below 1", boxAspect: 1.4, maxTex: 16384, devicePixelHeight: 0, expected: 1 },
    { name: "a degenerate GPU limit never gives less than 1", boxAspect: 1.4, maxTex: 0, devicePixelHeight: 2880, expected: 1 },
  ];

  it.each(cases)("$name", ({ expected, ...limits }) => {
    expect(textureHeight(limits)).toBe(expected);
  });

  it("never exceeds TEX_HEIGHT_MAX", () => {
    expect(textureHeight({ boxAspect: 3, maxTex: 32768, devicePixelHeight: 100000 })).toBeLessThanOrEqual(TEX_HEIGHT_MAX);
  });

  it("keeps twelve cached pages, mipmaps included, within the budget", () => {
    for (const boxAspect of [0.25, 0.5, 0.72, 1, 1.4, 2]) {
      const texH = textureHeight({ boxAspect, maxTex: 16384, devicePixelHeight: 4000 });
      const texW = Math.round(texH / boxAspect);
      const bytes = 12 * texW * texH * 4 * (4 / 3);
      expect(bytes).toBeLessThanOrEqual(TEX_BUDGET * 1.01);
    }
  });

  it("keeps the texture width within the GPU limit", () => {
    for (const maxTex of [1024, 2048, 4096]) {
      for (const boxAspect of [0.3, 0.5, 1, 1.4]) {
        const texH = textureHeight({ boxAspect, maxTex, devicePixelHeight: 4000 });
        expect(Math.round(texH / boxAspect)).toBeLessThanOrEqual(maxTex);
      }
    }
  });
});
