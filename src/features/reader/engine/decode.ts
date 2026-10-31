// Page decoding types and the main-thread decoder. A `PageDecoder` is the seam the texture cache
// loads through: the worker client (decoderClient.ts) returns pages already letterboxed at
// texture size, and the main-thread decoder here returns the raw scan for the backend to
// letterbox with `composePage`. The worker client falls back to this one where a worker cannot
// compose.

import { paintPage, textureWidth } from "./compose";
import type { Rgb } from "./scene";

/** One page of the book: where to fetch it and its pixel size from the book's metadata. */
export type PageSource = { url: string; width: number; height: number };

/**
 * A decoded page. With `composed` false it is the raw scan, and the backend letterboxes it. With
 * `composed` true it is already letterboxed onto paper at texture size and is uploaded as is.
 * The consumer closes the bitmap once it has used it.
 */
export type DecodedPage = { bitmap: ImageBitmap; width: number; height: number; composed: boolean };

/** What a decoder letterboxes onto: the page box, the texture height, and the paper colour. */
export type ComposeTarget = { boxAspect: number; texH: number; paper: Rgb };

/**
 * Fetches and decodes one page. A decoder may return the raw scan or a page composed to `target`.
 * It rejects with the abort reason when the signal fires.
 */
export type PageDecoder = (page: PageSource, target: ComposeTarget, signal: AbortSignal) => Promise<DecodedPage>;

/** Fetches and decodes one scan on the main thread, with no letterbox. */
export async function decodeRaw(page: PageSource, signal: AbortSignal): Promise<DecodedPage> {
  const res = await fetch(page.url, { signal });
  // The URL holds a local path, so the error carries only the status.
  if (!res.ok) throw new Error(`page fetch failed: HTTP ${res.status}`);
  const bitmap = await createImageBitmap(await res.blob());
  if (signal.aborted) {
    bitmap.close();
    signal.throwIfAborted();
  }
  return { bitmap, width: bitmap.width, height: bitmap.height, composed: false };
}

/** The main-thread `PageDecoder`: the raw scan, which the backend composes. */
export const decodePage: PageDecoder = (page, _target, signal) => decodeRaw(page, signal);

/**
 * Draws the scan letterboxed onto paper at texture resolution on a DOM canvas. texH is the
 * texture height. With no page it returns a small plain-paper canvas.
 */
export function composePage(page: DecodedPage | null, boxAspect: number, texH: number, paper: Rgb): HTMLCanvasElement {
  const texW = textureWidth(texH, boxAspect);
  const c = document.createElement("canvas");
  c.width = page ? texW : 4;
  c.height = page ? texH : 4;
  const scan = page ? { image: page.bitmap, width: page.width, height: page.height } : null;
  paintPage(c.getContext("2d")!, scan, boxAspect, c.width, c.height, paper);
  return c;
}
