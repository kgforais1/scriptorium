// Thin WebGL2 helpers: programs, meshes, textures. Everything created here is tracked by
// the owning engine and deleted on dispose, because React StrictMode remounts reuse the
// same canvas and therefore the same context.

import type { MeshData } from "./geometry";

export type Program = { p: WebGLProgram; u: Record<string, WebGLUniformLocation | null> };
export type Mesh = { vao: WebGLVertexArrayObject; vb: WebGLBuffer; ib: WebGLBuffer; n: number; type: number; len?: number };

function compile(gl: WebGL2RenderingContext, type: number, src: string) {
  const s = gl.createShader(type)!;
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(s);
    gl.deleteShader(s);
    throw new Error(`shader compile failed: ${log}`);
  }
  return s;
}

export function program(gl: WebGL2RenderingContext, vs: string, fs: string): Program {
  const p = gl.createProgram()!;
  const a = compile(gl, gl.VERTEX_SHADER, vs), b = compile(gl, gl.FRAGMENT_SHADER, fs);
  gl.attachShader(p, a);
  gl.attachShader(p, b);
  gl.linkProgram(p);
  gl.deleteShader(a);
  gl.deleteShader(b);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(`program link failed: ${gl.getProgramInfoLog(p)}`);
  const u: Program["u"] = {};
  const n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS) as number;
  for (let i = 0; i < n; i++) {
    const info = gl.getActiveUniform(p, i)!;
    u[info.name.replace("[0]", "")] = gl.getUniformLocation(p, info.name);
  }
  return { p, u };
}

export function mesh(gl: WebGL2RenderingContext, data: { V: number[] | Float32Array; I: number[] }, dynamic = false): Mesh {
  const verts = data.V instanceof Float32Array ? data.V : new Float32Array(data.V);
  const vao = gl.createVertexArray()!;
  gl.bindVertexArray(vao);
  const vb = gl.createBuffer()!;
  gl.bindBuffer(gl.ARRAY_BUFFER, vb);
  gl.bufferData(gl.ARRAY_BUFFER, verts, dynamic ? gl.DYNAMIC_DRAW : gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 32, 0);
  gl.enableVertexAttribArray(1);
  gl.vertexAttribPointer(1, 3, gl.FLOAT, false, 32, 12);
  gl.enableVertexAttribArray(2);
  gl.vertexAttribPointer(2, 2, gl.FLOAT, false, 32, 24);
  const ib = gl.createBuffer()!;
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ib);
  const wide = data.I.length > 65535 || verts.length / 8 > 65535;
  const idx = wide ? new Uint32Array(data.I) : new Uint16Array(data.I);
  gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, idx, gl.STATIC_DRAW);
  gl.bindVertexArray(null);
  return { vao, vb, ib, n: idx.length, type: wide ? gl.UNSIGNED_INT : gl.UNSIGNED_SHORT };
}

export function meshFrom(gl: WebGL2RenderingContext, data: MeshData & { len?: number }): Mesh {
  const m = mesh(gl, data);
  if (data.len !== undefined) m.len = data.len;
  return m;
}

export function freeMesh(gl: WebGL2RenderingContext, m: Mesh | null | undefined) {
  if (!m) return;
  gl.deleteBuffer(m.vb);
  gl.deleteBuffer(m.ib);
  gl.deleteVertexArray(m.vao);
}

export function drawMesh(gl: WebGL2RenderingContext, m: Mesh) {
  gl.bindVertexArray(m.vao);
  gl.drawElements(gl.TRIANGLES, m.n, m.type, 0);
}

export function textureFrom(gl: WebGL2RenderingContext, src: TexImageSource): WebGLTexture {
  const t = gl.createTexture()!;
  gl.bindTexture(gl.TEXTURE_2D, t);
  gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
  gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, src);
  gl.generateMipmap(gl.TEXTURE_2D);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  const aniso = gl.getExtension("EXT_texture_filter_anisotropic");
  if (aniso) gl.texParameterf(gl.TEXTURE_2D, aniso.TEXTURE_MAX_ANISOTROPY_EXT, Math.min(8, gl.getParameter(aniso.MAX_TEXTURE_MAX_ANISOTROPY_EXT)));
  return t;
}

export function fullscreenTriangle(gl: WebGL2RenderingContext): [WebGLVertexArrayObject, WebGLBuffer] {
  const vao = gl.createVertexArray()!;
  gl.bindVertexArray(vao);
  const buf = gl.createBuffer()!;
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.bindVertexArray(null);
  return [vao, buf];
}
