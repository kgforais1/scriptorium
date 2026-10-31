// Page texture size. A texture is `texH / boxAspect` wide, so a page costs 4 * texH^2 / boxAspect
// bytes, plus a third for mipmaps. The cache keeps TEX_CACHE pages, so
// 12 * 4 * (4/3) * texH^2 / boxAspect = 64 * texH^2 / boxAspect must fit TEX_BUDGET. Pure.

/** The tallest page texture, whatever the display and the budget allow. */
export const TEX_HEIGHT_MAX = 2048;

/** Memory the cached page textures may use in total: 256 MB. */
export const TEX_BUDGET = 256 * 1024 * 1024;

/** What the texture height depends on. */
export type TextureLimits = {
  /** The page box's aspect, height over width. */
  boxAspect: number;
  /** The GPU's MAX_TEXTURE_SIZE. */
  maxTex: number;
  /** The drawing buffer's height in device pixels: the display, as the engine sees it. */
  devicePixelHeight: number;
};

/**
 * The texture height for new uploads: the smallest of the display height, 2048, the memory
 * budget's limit, the GPU maximum, and the height at which the width still fits the GPU maximum.
 * Never less than 1.
 */
export function textureHeight({ boxAspect, maxTex, devicePixelHeight }: Readonly<TextureLimits>): number {
  const byBudget = Math.floor(Math.sqrt((TEX_BUDGET * boxAspect) / 64));
  const byWidth = Math.floor(maxTex * boxAspect);
  return Math.max(1, Math.floor(Math.min(devicePixelHeight, TEX_HEIGHT_MAX, byBudget, maxTex, byWidth)));
}
