import { afterEach, describe, expect, it, vi } from "vitest";
import type { DecodedPage } from "./decode";
import { createGlBackend, imageForUpload } from "./glBackend";

afterEach(() => vi.unstubAllGlobals());

const bitmap = { close: vi.fn(), tag: "bitmap" } as unknown as ImageBitmap;
const raw: DecodedPage = { bitmap, width: 200, height: 100, composed: false };
const done: DecodedPage = { bitmap, width: 1000, height: 1000, composed: true };

// A stand-in GL context: constants are numbers, getError is clean, and every call is recorded.
function fakeGl() {
  const calls: Record<string, unknown[][]> = {};
  const gl = new Proxy(
    {},
    {
      get(_t, prop: string) {
        if (prop === "getError") return () => 0;
        if (prop === "NO_ERROR") return 0;
        if (prop === "getExtension") return () => null;
        if (prop === "getParameter") return () => 4096;
        if (/^[A-Z][A-Z0-9_]*$/.test(prop)) return 1;
        return (...args: unknown[]) => {
          if (!calls[prop]) calls[prop] = [];
          calls[prop].push(args);
          return prop === "createTexture" ? { id: calls[prop].length } : undefined;
        };
      },
    },
  ) as unknown as WebGL2RenderingContext;
  return { gl, calls };
}

function stubCanvas() {
  const ctx = { fillStyle: "", fillRect: vi.fn(), drawImage: vi.fn(), imageSmoothingQuality: "" };
  const canvas = { width: 0, height: 0, getContext: () => ctx };
  const createElement = vi.fn(() => canvas);
  vi.stubGlobal("document", { createElement });
  return { ctx, canvas, createElement };
}

describe("imageForUpload", () => {
  it("returns a composed bitmap as is and never touches a canvas", () => {
    // No document is stubbed: composing would throw.
    expect(imageForUpload(done, 1.4, 512, [1, 1, 1])).toBe(bitmap);
  });

  it("letterboxes a raw scan at the given height", () => {
    const { ctx, canvas } = stubCanvas();
    const image = imageForUpload(raw, 1, 1000, [1, 1, 1]);
    expect(image).toBe(canvas);
    expect([canvas.width, canvas.height]).toEqual([1000, 1000]);
    expect(ctx.drawImage).toHaveBeenCalledWith(bitmap, 0, 250, 1000, 500);
  });
});

describe("createGlBackend", () => {
  it("uploads a composed page's bitmap directly and skips the 2D canvas", () => {
    const { gl, calls } = fakeGl();
    const backend = createGlBackend(gl, 1.4);
    backend.page(done, [1, 1, 1], 512);
    expect(calls.texImage2D).toHaveLength(1);
    expect(calls.texImage2D[0][calls.texImage2D[0].length - 1]).toBe(bitmap);
  });

  it("composes a raw scan on a canvas, at the height it is given, then uploads the canvas", () => {
    const { gl, calls } = fakeGl();
    const { canvas, createElement } = stubCanvas();
    createGlBackend(gl, 1).page(raw, [1, 1, 1], 640);
    expect(createElement).toHaveBeenCalledWith("canvas");
    expect(canvas.height).toBe(640);
    expect(calls.texImage2D[0][calls.texImage2D[0].length - 1]).toBe(canvas);
  });

  it("builds the paper texture from a 4 px canvas", () => {
    const { gl, calls } = fakeGl();
    const { canvas } = stubCanvas();
    createGlBackend(gl, 1.4).paper([0.9, 0.9, 0.8]);
    expect([canvas.width, canvas.height]).toEqual([4, 4]);
    expect(calls.texImage2D).toHaveLength(1);
  });

  it("frees a texture", () => {
    const { gl, calls } = fakeGl();
    const tex = {} as WebGLTexture;
    createGlBackend(gl, 1.4).free(tex);
    expect(calls.deleteTexture[0][0]).toBe(tex);
  });
});
