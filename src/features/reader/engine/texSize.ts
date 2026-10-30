// Page texture size. Chunk 2 of M1 Phase 3 replaces this with the display- and budget-aware
// choice; for now it is the spike's fixed height, clamped to what the GPU accepts.

/** The spike's fixed texture height. */
export const TEX_HEIGHT = 2048;

/**
 * The texture height for a page box of aspect boxAspect (height over width). A texture is
 * `height / boxAspect` wide, so a wide book lowers the height to keep both sides within maxTex.
 */
export function textureHeight(boxAspect: number, maxTex: number): number {
  return Math.max(1, Math.floor(Math.min(TEX_HEIGHT, maxTex, maxTex * boxAspect)));
}
