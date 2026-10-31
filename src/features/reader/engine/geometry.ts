// Book geometry, ported from mockup E. Pure functions over plain arrays, no WebGL.
// World units: one page is W = 1.0 wide; H comes from the book's page aspect.

import { PI, clamp, lerp, smooth, v3, type Vec3 } from "./math";

export const NU = 48;
export const NV = 28;
const LG = 0.085;
const R0 = 0.02;
const DROOP = 0.035;
const EPS = 0.0022;

export type Book = {
  W: number;
  H: number;
  MRG: number; // board margin around the page block
  BT: number; // board thickness
  ZG: number; // gutter height
  PT: number; // per-leaf thickness, scaled so long books stay a sane height
};

/** Book dimensions for a page box of the given aspect (height over width) and leaf count. */
export function makeBook(aspect: number, leaves: number): Book {
  const BT = 0.034;
  return { W: 1, H: aspect, MRG: 0.048, BT, ZG: BT + 0.004, PT: Math.min(0.0042, 0.16 / Math.max(1, leaves)) };
}

export type MeshData = { V: number[]; I: number[] };

/** Collects vertices and indices for one mesh. */
export function builder() {
  const V: number[] = [];
  const I: number[] = [];
  return {
    V,
    I,
    v(p: Vec3, n: Vec3, u: number, w: number) {
      V.push(p[0], p[1], p[2], n[0], n[1], n[2], u, w);
      return V.length / 8 - 1;
    },
    quad(pts: number[][], n: Vec3) {
      const i = V.length / 8;
      for (const p of pts) V.push(p[0], p[1], p[2], n[0], n[1], n[2], p[3] || 0, p[4] || 0);
      I.push(i, i + 1, i + 2, i, i + 2, i + 3);
    },
    data(): MeshData {
      return { V, I };
    },
  };
}

/** Leather board with chamfered top edges. */
export function boardMesh(b: Book): MeshData {
  const m = builder();
  const hx = b.W + b.MRG, hy = b.H / 2 + b.MRG, zt = b.BT, bv = 0.007, zb = zt - bv, ix = hx - bv, iy = hy - bv, s = Math.SQRT1_2;
  m.quad([[-ix, -iy, zt], [ix, -iy, zt], [ix, iy, zt], [-ix, iy, zt]], [0, 0, 1]);
  m.quad([[ix, -iy, zt], [hx, -hy, zb], [hx, hy, zb], [ix, iy, zt]], [s, 0, s]);
  m.quad([[-ix, iy, zt], [-hx, hy, zb], [-hx, -hy, zb], [-ix, -iy, zt]], [-s, 0, s]);
  m.quad([[-ix, iy, zt], [ix, iy, zt], [hx, hy, zb], [-hx, hy, zb]], [0, s, s]);
  m.quad([[-hx, -hy, zb], [hx, -hy, zb], [ix, -iy, zt], [-ix, -iy, zt]], [0, -s, s]);
  m.quad([[hx, -hy, 0], [hx, hy, 0], [hx, hy, zb], [hx, -hy, zb]], [1, 0, 0]);
  m.quad([[-hx, hy, 0], [-hx, -hy, 0], [-hx, -hy, zb], [-hx, hy, zb]], [-1, 0, 0]);
  m.quad([[-hx, -hy, 0], [hx, -hy, 0], [hx, -hy, zb], [-hx, -hy, zb]], [0, -1, 0]);
  m.quad([[hx, hy, 0], [-hx, hy, 0], [-hx, hy, zb], [hx, hy, zb]], [0, 1, 0]);
  return m.data();
}

/** Desk plane under the book. */
export function tableMesh(): MeshData {
  const m = builder();
  m.quad([[-9, -7, 0], [9, -7, 0], [9, 7, 0], [-9, 7, 0]], [0, 0, 1]);
  return m.data();
}

const stackT = (b: Book, n: number) => 0.006 + n * b.PT;
/** Gutter lift angle for a stack of n leaves. */
export const gutterLift = (b: Book, n: number) => Math.min(1.25, (stackT(b, n) + R0) / LG);

export type Profile = { X: Float32Array; Z: Float32Array };

/** Resting profile of the top leaf of a stack of n leaves (the gutter curve). */
export function restProfile(b: Book, n: number): Profile {
  const g = gutterLift(b, n), X = new Float32Array(NU), Z = new Float32Array(NU), ds = b.W / (NU - 1);
  X[0] = 0;
  Z[0] = b.ZG;
  for (let i = 1; i < NU; i++) {
    const s = (i - 0.5) * ds, th = g * Math.exp(-s / LG) - DROOP * (s / b.W);
    X[i] = X[i - 1] + Math.cos(th) * ds;
    Z[i] = Z[i - 1] + Math.sin(th) * ds;
  }
  return { X, Z };
}

type Builder = ReturnType<typeof builder>;

// The page block's top surface: a rows x NU grid following the gutter profile.
function stackTop(b: Book, sig: number, P: Profile, top: Builder) {
  const rows = 10;
  for (let j = 0; j < rows; j++) {
    const vn = j / (rows - 1), y = b.H / 2 - vn * b.H;
    for (let i = 0; i < NU; i++) {
      const i0 = Math.max(0, i - 1), i1 = Math.min(NU - 1, i + 1);
      const dx = P.X[i1] - P.X[i0], dz = P.Z[i1] - P.Z[i0], l = Math.hypot(dx, dz), s01 = i / (NU - 1);
      top.v([sig * P.X[i], y, P.Z[i]], [(-sig * dz) / l, 0, dx / l], sig > 0 ? s01 : 1 - s01, vn);
    }
  }
  for (let j = 0; j < rows - 1; j++)
    for (let i = 0; i < NU - 1; i++) {
      const a = j * NU + i, c = a + NU;
      top.I.push(a, c, a + 1, a + 1, c, c + 1);
    }
}

// The page block's outer edge and its head and tail faces.
function stackEdges(b: Book, sig: number, P: Profile, layers: number, ed: Builder) {
  const xe = sig * P.X[NU - 1], ze = P.Z[NU - 1];
  ed.quad([[xe, -b.H / 2, b.BT, 0, 0], [xe, b.H / 2, b.BT, 1, 0], [xe, b.H / 2, ze, 1, layers], [xe, -b.H / 2, ze, 0, layers]], [sig, 0, 0]);
  for (const yy of [-b.H / 2, b.H / 2]) {
    const base = ed.V.length / 8, ny = yy < 0 ? -1 : 1;
    for (let i = 0; i < NU; i++) {
      ed.v([sig * P.X[i], yy, P.Z[i]], [0, ny, 0], i / (NU - 1), layers);
      ed.v([sig * P.X[i], yy, b.BT], [0, ny, 0], i / (NU - 1), 0);
    }
    for (let i = 0; i < NU - 1; i++) {
      const a = base + i * 2;
      ed.I.push(a, a + 1, a + 2, a + 2, a + 1, a + 3);
    }
  }
}

/** Top surface and edges of one side's page block. sig is -1 for left, +1 for right. */
export function stackMeshes(b: Book, sig: number, n: number) {
  const P = restProfile(b, n), top = builder(), ed = builder();
  stackTop(b, sig, P, top);
  stackEdges(b, sig, P, n * 5, ed);
  return { top: top.data(), edge: ed.data(), prof: P };
}

export type TurnKind = "peek" | "drag" | "anim";

export type Turn = {
  dir: 1 | -1;
  leaf: number;
  t: number; // 0 = lying on the right, 1 = lying on the left
  kind: TurnKind;
  cv: number; // grip height, 0 top to 1 bottom
  A0: number; // how much corner lift the grip implies
  pk: number;
  pkT: number;
  lift: number;
  cancel: number;
  gA: number;
  gB: number;
  anim: { t0: number; t1: number; el: number; dur: number; ease: (x: number) => number } | null;
};

function bendAngle(s01: number, s: number, t: number, gA: number, gB: number) {
  const ts = Math.pow(t, 1.35);
  const te = clamp(t + 0.36 * Math.sin(PI * t) * (1 - 1.45 * t), 0, 1);
  const thS = PI * ts, thE = PI * te;
  const f = s01 * (1.55 - 0.55 * s01);
  let th = thS + (thE - thS) * f;
  const ex = Math.exp(-s / LG), rA = gA * ex - DROOP * s01, rB = gB * ex - DROOP * s01;
  th += rA * (1 - ts) - rB * ts;
  return clamp(th, rA * (1 - ts), PI - rB * ts);
}

function curlOf(b: Book, T: Turn, tp: number) {
  const yc = b.H / 2 - T.cv * b.H, corner = smooth(0.16, 0.42, Math.abs(T.cv - 0.5));
  let dx = -1, dy = -Math.sign(yc) * 0.8 * corner;
  const l = Math.hypot(dx, dy);
  dx /= l;
  dy /= l;
  const A = clamp(T.A0 / 1.3, 0, 1);
  let Dq: number, phi: number;
  if (T.kind === "peek") {
    Dq = 0.25 * T.pk;
    phi = 2.0 * T.pk;
  } else {
    const grow = smooth(0, 0.32, tp), fade = 1 - smooth(0.42, 0.9, tp);
    let k = T.lift * fade;
    if (T.cancel) k *= smooth(0, T.cancel, tp);
    Dq = (0.21 + 0.3 * grow) * k * (0.45 + 0.55 * A);
    phi = (1.9 - 0.6 * grow) * k * (0.3 + 0.7 * A);
  }
  return { yc, dx, dy, Dq, phi };
}

export const LEAF_FLOATS = NU * NV * 8;

/** Triangle indices for the turning leaf grid. */
export function leafIndices(): number[] {
  const out: number[] = [];
  for (let j = 0; j < NV - 1; j++)
    for (let i = 0; i < NU - 1; i++) {
      const a = j * NU + i, c = a + NU;
      out.push(a, c, a + 1, a + 1, c, c + 1);
    }
  return out;
}

const prX = new Float32Array(NU), prZ = new Float32Array(NU), prA = new Float32Array(NU);

// Fills the shared bend profile (position and angle per column) for the leaf at progress tp.
function leafProfile(b: Book, T: Turn, tp: number) {
  const ds = b.W / (NU - 1), fwd = T.dir > 0;
  let x = 0, z = b.ZG + EPS;
  prX[0] = 0;
  prZ[0] = z;
  for (let i = 1; i < NU; i++) {
    let th = bendAngle((i - 0.5) / (NU - 1), (i - 0.5) * ds, tp, T.gA, T.gB);
    if (!fwd) th = PI - th;
    prA[i] = th;
    x += Math.cos(th) * ds;
    z += Math.sin(th) * ds;
    prX[i] = x;
    prZ[i] = z;
  }
  prA[0] = prA[1];
}

// Writes one vertex row of the leaf and the row's footprint (min x, max x, peak height) into foot.
function leafRow(b: Book, fwd: boolean, curl: Curl | null, j: number, leafV: Float32Array, foot: Float32Array) {
  const ds = b.W / (NU - 1), vn = j / (NV - 1), y = b.H / 2 - vn * b.H;
  let mn = 1e9, mx = -1e9, hz = 0;
  for (let i = 0; i < NU; i++) {
    const { sp, yp, h } = curlPoint(curl, b.W, i * ds, y);
    const fi = clamp(sp / ds, 0, NU - 1), i0 = Math.min(NU - 2, Math.floor(fi)), f = fi - i0;
    const X = lerp(prX[i0], prX[i0 + 1], f), Z = lerp(prZ[i0], prZ[i0 + 1], f), th = lerp(prA[i0], prA[i0 + 1], f);
    const nx = fwd ? -Math.sin(th) : Math.sin(th), nz = fwd ? Math.cos(th) : -Math.cos(th);
    const px = X + nx * h, pz = Z + nz * h;
    const o = (j * NU + i) * 8;
    leafV[o] = px;
    leafV[o + 1] = yp;
    leafV[o + 2] = pz;
    leafV[o + 6] = i / (NU - 1);
    leafV[o + 7] = vn;
    mn = Math.min(mn, px);
    mx = Math.max(mx, px);
    hz = Math.max(hz, pz);
  }
  foot[j * 4] = mn;
  foot[j * 4 + 1] = mx;
  foot[j * 4 + 2] = Math.max(0, hz - b.ZG - 0.055);
  foot[j * 4 + 3] = 0;
}

/** Writes the turning leaf's vertices into leafV and the per-row footprint (for the cast shadow) into foot. */
export function updateLeaf(b: Book, T: Turn, leafV: Float32Array, foot: Float32Array) {
  const tp = T.dir > 0 ? T.t : 1 - T.t;
  leafProfile(b, T, tp);
  const C = curlOf(b, T, tp), curl = C.Dq > 1e-4 && C.phi > 1e-3 ? C : null;
  for (let j = 0; j < NV; j++) leafRow(b, T.dir > 0, curl, j, leafV, foot);
  leafNormals(leafV);
}

type Curl = ReturnType<typeof curlOf>;

// Wraps material point (s, y) round the corner-lift cylinder. Returns its new material position
// along the bend profile and its height off the unlifted surface.
function curlPoint(C: Curl | null, W: number, s: number, y: number) {
  if (!C) return { sp: s, yp: y, h: 0 };
  const q = (s - W) * C.dx + (y - C.yc) * C.dy, a = C.Dq - q;
  if (a <= 0) return { sp: s, yp: y, h: 0 };
  const r = C.Dq / C.phi, ph = a / r;
  const rolled = ph < PI;
  const qn = rolled ? C.Dq - r * Math.sin(ph) : C.Dq + (a - PI * r);
  const h = rolled ? r * (1 - Math.cos(ph)) : 2 * r;
  const sh = qn - q;
  return { sp: s + C.dx * sh, yp: y + C.dy * sh, h };
}

// Per-vertex normals from central differences across the leaf grid.
function leafNormals(leafV: Float32Array) {
  const p = (jj: number, ii: number) => (jj * NU + ii) * 8;
  for (let j = 0; j < NV; j++)
    for (let i = 0; i < NU; i++) {
      const a = p(j, Math.max(0, i - 1)), c = p(j, Math.min(NU - 1, i + 1));
      const d0 = p(Math.max(0, j - 1), i), d1 = p(Math.min(NV - 1, j + 1), i);
      const du: Vec3 = [leafV[c] - leafV[a], leafV[c + 1] - leafV[a + 1], leafV[c + 2] - leafV[a + 2]];
      const dv: Vec3 = [leafV[d1] - leafV[d0], leafV[d1 + 1] - leafV[d0 + 1], leafV[d1 + 2] - leafV[d0 + 2]];
      const n = v3.norm(v3.cross(dv, du)), o = p(j, i);
      leafV[o + 3] = n[0];
      leafV[o + 4] = n[1];
      leafV[o + 5] = n[2];
    }
}

function catmull(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: number): Vec3 {
  const t2 = t * t, t3 = t2 * t;
  const k = (i: number) =>
    0.5 * (2 * p1[i] + (p2[i] - p0[i]) * t + (2 * p0[i] - 5 * p1[i] + 4 * p2[i] - p3[i]) * t2 + (3 * p1[i] - p0[i] - 3 * p2[i] + p3[i]) * t3);
  return [k(0), k(1), k(2)];
}

/** Ribbon bookmark: runs down the gutter, out of the tail of the block, and onto the desk. */
export function ribbonMesh(b: Book, width: number): MeshData & { len: number } {
  // The ribbon starts 0.19 above the tail, clamped onto the board for very short pages.
  const YB = b.H / 2 + b.MRG, top = Math.min(-b.H / 2 + 0.19, b.H / 2);
  const pts: Vec3[] = [
    [0.003, top, b.ZG - 0.014], [0.004, top - 0.16, b.ZG - 0.01], [0.006, -YB + 0.033, b.BT + 0.004],
    [0.009, -YB + 0.006, b.BT + 0.001], [0.013, -YB - 0.006, 0.02], [0.018, -YB - 0.016, 0.0035],
    [0.03, -YB - 0.05, 0.0015], [0.06, -YB - 0.11, 0.0015], [0.085, -YB - 0.17, 0.0015], [0.088, -YB - 0.23, 0.0015],
  ];
  const P: Vec3[] = [];
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[Math.max(0, i - 1)], p1 = pts[i], p2 = pts[i + 1], p3 = pts[Math.min(pts.length - 1, i + 2)];
    for (let k = 0; k < 12; k++) P.push(catmull(p0, p1, p2, p3, k / 12));
  }
  P.push(pts[pts.length - 1]);
  const m = builder();
  let len = 0;
  for (let i = 0; i < P.length; i++) {
    if (i) len += v3.len(v3.sub(P[i], P[i - 1]));
    const T = v3.norm(v3.sub(P[Math.min(P.length - 1, i + 1)], P[Math.max(0, i - 1)]));
    const tw = 0.18 * Math.sin(i * 0.09);
    let S = v3.norm(v3.cross(T, [0, 0, 1]));
    S = v3.norm(v3.add(S, v3.scale([0, 0, 1], tw * 0.3)));
    const N = v3.norm(v3.cross(S, T));
    const a = v3.add(P[i], v3.scale(S, width / 2)), c = v3.sub(P[i], v3.scale(S, width / 2));
    m.v(a, N, len, 1);
    m.v(c, N, len, -1);
    if (i) {
      const q = (i - 1) * 2;
      m.I.push(q, q + 1, q + 2, q + 2, q + 1, q + 3);
    }
  }
  return { ...m.data(), len };
}
