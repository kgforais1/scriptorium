import { describe, expect, it, vi } from "vitest";
import { paintPage, paperCss, textureWidth, type Paint2D } from "./compose";

function ctx() {
  return { fillStyle: "", fillRect: vi.fn(), drawImage: vi.fn(), imageSmoothingQuality: "low" } as unknown as Paint2D & {
    fillRect: ReturnType<typeof vi.fn>;
    drawImage: ReturnType<typeof vi.fn>;
  };
}

describe("textureWidth", () => {
  it("is the height over the box aspect, rounded", () => {
    expect(textureWidth(1400, 1.4)).toBe(1000);
    expect(textureWidth(1000, 3)).toBe(333);
  });

  it("is never less than 1, even for a tiny texture of a tall box", () => {
    expect(textureWidth(1, 2.1)).toBe(1);
    expect(textureWidth(2, 5)).toBe(1);
    expect(textureWidth(0, 1.4)).toBe(1);
  });
});

describe("paperCss", () => {
  it("scales channels to 0 to 255 and clamps", () => {
    expect(paperCss([1, 0.5, 0])).toBe("rgb(255,128,0)");
    expect(paperCss([2, -1, 0.2])).toBe("rgb(255,0,51)");
  });
});

describe("paintPage", () => {
  it("fills the surface with paper and draws nothing else when there is no scan", () => {
    const c = ctx();
    paintPage(c, null, 1.4, 4, 4, [1, 1, 1]);
    expect(c.fillStyle).toBe("rgb(255,255,255)");
    expect(c.fillRect).toHaveBeenCalledWith(0, 0, 4, 4);
    expect(c.drawImage).not.toHaveBeenCalled();
  });

  it("draws the scan fitted inside the page box at surface size", () => {
    const c = ctx();
    const image = {} as CanvasImageSource;
    // A wide scan in a square box: bands above and below.
    paintPage(c, { image, width: 200, height: 100 }, 1, 1000, 1000, [1, 1, 1]);
    expect(c.drawImage).toHaveBeenCalledWith(image, 0, 250, 1000, 500);
    expect(c.imageSmoothingQuality).toBe("high");
  });
});
