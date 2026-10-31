import { describe, expect, it } from "vitest";
import { computeCamera } from "./camera";
import {
  GEOMETRY_SLOTS, GEOMETRY_STRIDE, clearSlot, createGeometryBuffer, leafFace, projectInto, writeLeafSlot, writeStackSlot,
} from "./corners";
import { NU, NV, makeBook, restProfile } from "./geometry";
import { IDENTITY } from "./math";

const W = 1000, H = 800;
const T = { VP: IDENTITY, w: W, h: H };

describe("projectInto", () => {
  it("maps the clip-space origin to the canvas centre and flips y", () => {
    const out = new Float32Array(2);
    projectInto(out, 0, T, 0, 0, 0);
    expect([out[0], out[1]]).toEqual([500, 400]);
    projectInto(out, 0, T, -1, 1, 0);
    expect([out[0], out[1]]).toEqual([0, 0]);
    projectInto(out, 0, T, 1, -1, 0);
    expect([out[0], out[1]]).toEqual([W, H]);
  });
});

describe("geometry buffer", () => {
  it("has a fixed size, so publishing allocates nothing", () => {
    expect(createGeometryBuffer()).toHaveLength(GEOMETRY_SLOTS * GEOMETRY_STRIDE);
  });

  it("marks an unused slot with page -1 and zero corners", () => {
    const buf = createGeometryBuffer().fill(7);
    clearSlot(buf, 1);
    expect(buf[GEOMETRY_STRIDE]).toBe(-1);
    expect(Array.from(buf.slice(GEOMETRY_STRIDE + 1, 2 * GEOMETRY_STRIDE))).toEqual(new Array(8).fill(0));
    expect(buf[0]).toBe(7);
  });
});

describe("writeStackSlot", () => {
  const book = makeBook(1.4, 10);
  const cam = computeCamera(W, H, book, 18);
  const t = { VP: cam.VP, w: W, h: H };
  const prof = restProfile(book, 8);

  it("writes four corners in reading order for each block", () => {
    const buf = createGeometryBuffer();
    writeStackSlot(buf, 0, 4, prof, -1, book, t);
    writeStackSlot(buf, 1, 5, prof, 1, book, t);
    const L = Array.from(buf.slice(0, GEOMETRY_STRIDE)), R = Array.from(buf.slice(GEOMETRY_STRIDE, 2 * GEOMETRY_STRIDE));
    expect(L[0]).toBe(4);
    expect(R[0]).toBe(5);
    // [page, TLx, TLy, TRx, TRy, BRx, BRy, BLx, BLy]
    expect(L[1]).toBeLessThan(L[3]);
    expect(L[2]).toBeLessThan(L[8]);
    expect(R[1]).toBeLessThan(R[3]);
    expect(R[2]).toBeLessThan(R[8]);
    // The gutter sits between the two blocks: the left block's top-right meets the right block's top-left.
    expect(L[3]).toBeCloseTo(R[1], 3);
    expect(L[4]).toBeCloseTo(R[2], 3);
    for (const v of buf.slice(0, 18)) expect(Number.isFinite(v)).toBe(true);
  });

  it("keeps the pages inside the canvas", () => {
    const buf = createGeometryBuffer();
    writeStackSlot(buf, 0, 0, prof, -1, book, t);
    writeStackSlot(buf, 1, 1, prof, 1, book, t);
    for (const s of [0, 1])
      for (let k = 0; k < 4; k++) {
        const x = buf[s * GEOMETRY_STRIDE + 1 + 2 * k], y = buf[s * GEOMETRY_STRIDE + 2 + 2 * k];
        expect(x).toBeGreaterThan(0);
        expect(x).toBeLessThan(W);
        expect(y).toBeGreaterThan(0);
        expect(y).toBeLessThan(H);
      }
  });
});

// A flat leaf: x runs 0..1 across the hinge-to-edge columns, y runs +0.7..-0.7 down the rows.
function flatLeaf(): Float32Array {
  const v = new Float32Array(NU * NV * 8);
  for (let j = 0; j < NV; j++)
    for (let i = 0; i < NU; i++) {
      const o = (j * NU + i) * 8;
      v[o] = i / (NU - 1);
      v[o + 1] = 0.7 - (1.4 * j) / (NV - 1);
      v[o + 2] = 0.05;
    }
  return v;
}


describe("writeLeafSlot", () => {
  const cam = computeCamera(W, H, makeBook(1.4, 4), 18);
  const t = { VP: cam.VP, w: W, h: H };

  it("reads the front face with the hinge on the left", () => {
    const buf = createGeometryBuffer();
    writeLeafSlot(buf, 2, 3, flatLeaf(), false, t);
    const o = 2 * GEOMETRY_STRIDE;
    expect(buf[o]).toBe(3);
    expect(buf[o + 1]).toBeLessThan(buf[o + 3]);
    expect(buf[o + 2]).toBeLessThan(buf[o + 6]);
  });

  it("reads the back face with the hinge on the right", () => {
    const buf = createGeometryBuffer();
    writeLeafSlot(buf, 2, 4, flatLeaf(), true, t);
    const o = 2 * GEOMETRY_STRIDE;
    // Top-left is the outer edge (x = 1), top-right is the hinge (x = 0).
    expect(buf[o + 1]).toBeGreaterThan(buf[o + 3]);
    expect(buf[o + 5]).toBeCloseTo(buf[o + 3], 3);
  });
});

describe("leafFace", () => {
  it("shows the front while the leaf lies on the right, in both directions", () => {
    expect(leafFace(3, 0)).toEqual({ back: false, slot: 7 });
    expect(leafFace(3, 0.49)).toEqual({ back: false, slot: 7 });
  });

  it("shows the back while the leaf lies on the left, including the start of a backward turn", () => {
    expect(leafFace(2, 1)).toEqual({ back: true, slot: 6 });
    expect(leafFace(2, 0.5)).toEqual({ back: true, slot: 6 });
  });
});
