// Page decoding on the main thread, behind the `PageDecoder` seam. M1 chunk 2 swaps this for a
// worker pipeline without touching the cache or the engine. Also the pad-to-uniform letterbox.

import { fitRect } from "./fit";
import type { Rgb } from "./scene";

/** One page of the book: where to fetch it and its pixel size from the book's metadata. */
export type PageSource = { url: string; width: number; height: number };

/** A decoded scan. The consumer closes the bitmap once it has used it. */
export type DecodedPage = { bitmap: ImageBitmap; width: number; height: number };

/** Fetches and decodes one page. Rejects with the abort reason when the signal fires. */
export type PageDecoder = (page: PageSource, signal: AbortSignal) => Promise<DecodedPage>;

/** The main-thread decoder: `fetch` plus `createImageBitmap`. */
export const decodePage: PageDecoder = async (page, signal) => {
  const res = await fetch(page.url, { signal });
  // The URL holds a local path, so the error carries only the status.
  if (!res.ok) throw new Error(`page fetch failed: HTTP ${res.status}`);
  const bitmap = await createImageBitmap(await res.blob());
  if (signal.aborted) {
    bitmap.close();
    signal.throwIfAborted();
  }
  return { bitmap, width: bitmap.width, height: bitmap.height };
};

const css = (c: Rgb) => `rgb(${c.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255)).join(",")})`;

/**
 * Draws the scan letterboxed onto paper at texture resolution. texH is the texture height. With
 * no page it returns a small plain-paper canvas.
 */
export function composePage(page: DecodedPage | null, boxAspect: number, texH: number, paper: Rgb): HTMLCanvasElement {
  const texW = Math.round(texH / boxAspect);
  const c = document.createElement("canvas");
  c.width = page ? texW : 4;
  c.height = page ? texH : 4;
  const x = c.getContext("2d")!;
  x.fillStyle = css(paper);
  x.fillRect(0, 0, c.width, c.height);
  if (!page) return c;
  const r = fitRect(page.width, page.height, boxAspect);
  x.imageSmoothingQuality = "high";
  x.drawImage(page.bitmap, r.x * texW, r.y * texH, r.w * texW, r.h * texH);
  return c;
}
