// Small vector and column-major 4x4 matrix helpers, ported from mockup E.

export type Vec3 = [number, number, number];
export type Mat4 = Float32Array;

/** Pi, shared so shader-side and CPU-side math agree. */
export const PI = Math.PI;
/** Limits x to the range a to b. */
export const clamp = (x: number, a: number, b: number) => Math.max(a, Math.min(b, x));
/** Linear blend from a to b. */
export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
/** Smoothstep of x between edges a and b. */
export const smooth = (a: number, b: number, x: number) => {
  const t = clamp((x - a) / (b - a), 0, 1);
  return t * t * (3 - 2 * t);
};
/** Cubic ease-in-out on 0 to 1. */
export const easeInOut = (x: number) => (x < 0.5 ? 4 * x * x * x : 1 - Math.pow(2 - 2 * x, 3) / 2);
/** Cubic ease-out on 0 to 1. */
export const easeOut = (x: number) => 1 - Math.pow(1 - x, 3);

/** Small 3-vector helpers. */
export const v3 = {
  /** a - b. */
  sub: (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]],
  /** a + b. */
  add: (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]],
  /** a times s. */
  scale: (a: Vec3, s: number): Vec3 => [a[0] * s, a[1] * s, a[2] * s],
  /** Dot product. */
  dot: (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2],
  /** Cross product. */
  cross: (a: Vec3, b: Vec3): Vec3 => [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ],
  /** Length. */
  len: (a: Vec3) => Math.hypot(a[0], a[1], a[2]),
  /** Unit vector (zero stays zero). */
  norm: (a: Vec3): Vec3 => {
    const l = Math.hypot(a[0], a[1], a[2]) || 1;
    return [a[0] / l, a[1] / l, a[2] / l];
  },
};

/** The 4x4 identity matrix. Treat as read-only. */
export const IDENTITY: Mat4 = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);

/** Column-major perspective projection matrix. */
export function perspective(fov: number, aspect: number, near: number, far: number): Mat4 {
  const t = 1 / Math.tan(fov / 2);
  return new Float32Array([
    t / aspect, 0, 0, 0,
    0, t, 0, 0,
    0, 0, (far + near) / (near - far), -1,
    0, 0, (2 * far * near) / (near - far), 0,
  ]);
}

/** Column-major view matrix looking from eye to center. */
export function lookAt(eye: Vec3, center: Vec3, up: Vec3): Mat4 {
  const z = v3.norm(v3.sub(eye, center));
  const x = v3.norm(v3.cross(up, z));
  const y = v3.cross(z, x);
  return new Float32Array([
    x[0], y[0], z[0], 0,
    x[1], y[1], z[1], 0,
    x[2], y[2], z[2], 0,
    -v3.dot(x, eye), -v3.dot(y, eye), -v3.dot(z, eye), 1,
  ]);
}

/** Product a * b of two column-major 4x4 matrices. */
export function multiply(a: Mat4, b: Mat4): Mat4 {
  const o = new Float32Array(16);
  for (let c = 0; c < 4; c++)
    for (let r = 0; r < 4; r++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
      o[c * 4 + r] = s;
    }
  return o;
}

/** Inverse of a 4x4 matrix, or all zeros when it is singular. */
export function invert(a: Mat4): Mat4 {
  const o = new Float32Array(16);
  const [a00, a01, a02, a03, a10, a11, a12, a13, a20, a21, a22, a23, a30, a31, a32, a33] = a;
  const b00 = a00 * a11 - a01 * a10, b01 = a00 * a12 - a02 * a10, b02 = a00 * a13 - a03 * a10;
  const b03 = a01 * a12 - a02 * a11, b04 = a01 * a13 - a03 * a11, b05 = a02 * a13 - a03 * a12;
  const b06 = a20 * a31 - a21 * a30, b07 = a20 * a32 - a22 * a30, b08 = a20 * a33 - a23 * a30;
  const b09 = a21 * a32 - a22 * a31, b10 = a21 * a33 - a23 * a31, b11 = a22 * a33 - a23 * a32;
  let d = b00 * b11 - b01 * b10 + b02 * b09 + b03 * b08 - b04 * b07 + b05 * b06;
  if (!d) return o;
  d = 1 / d;
  o[0] = (a11 * b11 - a12 * b10 + a13 * b09) * d;
  o[1] = (a02 * b10 - a01 * b11 - a03 * b09) * d;
  o[2] = (a31 * b05 - a32 * b04 + a33 * b03) * d;
  o[3] = (a22 * b04 - a21 * b05 - a23 * b03) * d;
  o[4] = (a12 * b08 - a10 * b11 - a13 * b07) * d;
  o[5] = (a00 * b11 - a02 * b08 + a03 * b07) * d;
  o[6] = (a32 * b02 - a30 * b05 - a33 * b01) * d;
  o[7] = (a20 * b05 - a22 * b02 + a23 * b01) * d;
  o[8] = (a10 * b10 - a11 * b08 + a13 * b06) * d;
  o[9] = (a01 * b08 - a00 * b10 - a03 * b06) * d;
  o[10] = (a30 * b04 - a31 * b02 + a33 * b00) * d;
  o[11] = (a21 * b02 - a20 * b04 - a23 * b00) * d;
  o[12] = (a11 * b07 - a10 * b09 - a12 * b06) * d;
  o[13] = (a00 * b09 - a01 * b07 + a02 * b06) * d;
  o[14] = (a31 * b01 - a30 * b03 - a32 * b00) * d;
  o[15] = (a20 * b03 - a21 * b01 + a22 * b00) * d;
  return o;
}

/** Applies a 4x4 matrix to a homogeneous vector. */
export function transform(m: Mat4, v: [number, number, number, number]): [number, number, number, number] {
  return [
    m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12] * v[3],
    m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13] * v[3],
    m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14] * v[3],
    m[3] * v[0] + m[7] * v[1] + m[11] * v[2] + m[15] * v[3],
  ];
}
