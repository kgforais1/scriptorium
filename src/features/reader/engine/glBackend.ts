// The WebGL texture backend for `PageTextures`: letterbox on a 2D canvas, upload with an error
// check. Excluded from coverage because it needs a GPU.

import { composePage, type DecodedPage } from "./decode";
import { uploadTexture } from "./gl";
import type { TextureBackend } from "./pageTextures";
import type { Rgb } from "./scene";
import { textureHeight } from "./texSize";

/** Textures for a book whose page box has the given aspect (height over width). */
export function createGlBackend(gl: WebGL2RenderingContext, boxAspect: number): TextureBackend<WebGLTexture> {
  const maxTex = gl.getParameter(gl.MAX_TEXTURE_SIZE) as number;
  return {
    paper: (paper: Rgb) => uploadTexture(gl, composePage(null, boxAspect, 4, paper)),
    page: (page: DecodedPage, paper: Rgb) =>
      uploadTexture(gl, composePage(page, boxAspect, textureHeight(boxAspect, maxTex), paper)),
    free: (tex) => gl.deleteTexture(tex),
  };
}
