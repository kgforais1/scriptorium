// Page decoding and the pad-to-uniform aspect policy. Each scan is fitted inside the book's
// page box with no stretch and no crop. The remaining band is filled with the skin's paper colour.

import type { Rgb } from "./scene";

export type DecodedPage = { bitmap: ImageBitmap; width: number; height: number };

export async function decodePage(url: string, signal: AbortSignal): Promise<DecodedPage> {
  const res = await fetch(url, { signal });
  // The URL holds a local path, so the error carries only the status.
  if (!res.ok) throw new Error(`page fetch failed: HTTP ${res.status}`);
  const bitmap = await createImageBitmap(await res.blob());
  if (signal.aborted) {
    bitmap.close();
    signal.throwIfAborted();
  }
  return { bitmap, width: bitmap.width, height: bitmap.height };
}

// Where a scan of size w x h sits inside a page box of aspect boxH / boxW, as fractions of the box.
export function fitRect(w: number, h: number, boxAspect: number) {
  const scanAspect = h / w;
  if (scanAspect > boxAspect) {
    const fw = boxAspect / scanAspect;
    return { x: (1 - fw) / 2, y: 0, w: fw, h: 1 };
  }
  const fh = scanAspect / boxAspect;
  return { x: 0, y: (1 - fh) / 2, w: 1, h: fh };
}

const css = (c: Rgb) => `rgb(${c.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255)).join(",")})`;

// Draws the scan letterboxed onto paper at texture resolution. texH is the texture height.
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
