// The pad-to-uniform letterbox drawing, shared by the main thread and the decode worker. It
// takes any 2D context, so it runs on a DOM canvas and on an OffscreenCanvas alike.

import { fitRect } from "./fit";
import type { Rgb } from "./scene";

/** The part of a 2D context the letterbox needs. DOM and Offscreen contexts both fit. */
export type Paint2D = Pick<CanvasRenderingContext2D, "fillStyle" | "fillRect" | "drawImage" | "imageSmoothingQuality">;

/** A scan to draw, with its pixel size. */
export type Scan = { image: CanvasImageSource; width: number; height: number };

/**
 * The width of a texture of height texH for a page box of aspect boxAspect. Never less than 1: a
 * zero-wide canvas cannot be turned into a bitmap or uploaded.
 */
export function textureWidth(texH: number, boxAspect: number): number {
  return Math.max(1, Math.round(texH / boxAspect));
}

/** A `rgb(...)` string for a 0 to 1 colour, with channels clamped. */
export function paperCss(c: Rgb): string {
  return `rgb(${c.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255)).join(",")})`;
}

/**
 * Fills a w x h surface with paper, then draws the scan fitted inside the page box with no
 * stretch and no crop. With no scan only the paper is drawn.
 */
export function paintPage(ctx: Paint2D, scan: Scan | null, boxAspect: number, w: number, h: number, paper: Rgb): void {
  ctx.fillStyle = paperCss(paper);
  ctx.fillRect(0, 0, w, h);
  if (!scan) return;
  const r = fitRect(scan.width, scan.height, boxAspect);
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(scan.image, r.x * w, r.y * h, r.w * w, r.h * h);
}
