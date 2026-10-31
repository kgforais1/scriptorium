// The WebGL texture backend for `PageTextures`: upload with an error check, after letterboxing a
// raw scan on a 2D canvas. A page the worker already composed is uploaded as is.

import { composePage, type DecodedPage } from "./decode";
import { uploadTexture } from "./gl";
import type { TextureBackend } from "./pageTextures";
import type { Rgb } from "./scene";

/**
 * The image to upload for a page: the bitmap itself when it is composed already, otherwise the
 * scan letterboxed onto paper at height texH.
 */
export function imageForUpload(page: DecodedPage, boxAspect: number, texH: number, paper: Rgb): TexImageSource {
  return page.composed ? page.bitmap : composePage(page, boxAspect, texH, paper);
}

/** Textures for a book whose page box has the given aspect (height over width). */
export function createGlBackend(gl: WebGL2RenderingContext, boxAspect: number): TextureBackend<WebGLTexture> {
  return {
    paper: (paper: Rgb) => uploadTexture(gl, composePage(null, boxAspect, 4, paper)),
    page: (page: DecodedPage, paper: Rgb, texH: number) => uploadTexture(gl, imageForUpload(page, boxAspect, texH, paper)),
    free: (tex) => gl.deleteTexture(tex),
  };
}
