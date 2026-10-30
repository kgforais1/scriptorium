import { describe, expect, it } from "vitest";
import {
  LEAF_FLOATS, NU, NV, boardMesh, gutterLift, leafIndices, makeBook, restProfile, ribbonMesh, stackMeshes, tableMesh,
  updateLeaf, type Turn,
} from "./geometry";

const book = makeBook(1.4, 10);

function turn(dir: 1 | -1, t: number, kind: Turn["kind"] = "anim"): Turn {
  return {
    dir, leaf: 0, t, kind, cv: 0.9, A0: 1.3, pk: 0.5, pkT: 1, lift: 1, cancel: 0,
    gA: gutterLift(book, 8), gB: gutterLift(book, 8), anim: null,
  };
}

describe("makeBook", () => {
  it("takes its height from the page aspect and thins leaves for long books", () => {
    expect(book.H).toBeCloseTo(1.4);
    expect(makeBook(1.4, 1000).PT).toBeLessThan(book.PT);
  });
});

describe("meshes", () => {
  it("builds indexed triangle meshes with eight floats per vertex", () => {
    const meshes = [boardMesh(book), tableMesh(), stackMeshes(book, 1, 5).top, stackMeshes(book, -1, 5).edge, ribbonMesh(book, 0.03)];
    for (const m of meshes) {
      expect(m.V.length % 8).toBe(0);
      expect(m.I.length % 3).toBe(0);
      const verts = m.V.length / 8;
      expect(Math.max(...m.I)).toBeLessThan(verts);
      expect(m.V.every(Number.isFinite)).toBe(true);
    }
  });

  it("starts the page profile at the gutter", () => {
    const p = restProfile(book, 5);
    expect(p.X[0]).toBe(0);
    expect(p.X[NU - 1]).toBeGreaterThan(0.9);
  });
});

describe("updateLeaf", () => {
  it("builds a finite leaf for every kind of turn, forward and backward", () => {
    const leafV = new Float32Array(LEAF_FLOATS), foot = new Float32Array(NV * 4);
    for (const dir of [1, -1] as const)
      for (const kind of ["anim", "peek", "drag"] as const)
        for (const t of [dir > 0 ? 0 : 1, 0.25, 0.5, 0.75, dir > 0 ? 1 : 0]) {
          updateLeaf(book, turn(dir, t, kind), leafV, foot);
          expect(leafV.every(Number.isFinite)).toBe(true);
          expect(foot.every(Number.isFinite)).toBe(true);
        }
  });

  it("lays a forward leaf on the right at t = 0 and on the left at t = 1", () => {
    const leafV = new Float32Array(LEAF_FLOATS), foot = new Float32Array(NV * 4);
    const tip = (j: number) => leafV[(j * NU + NU - 1) * 8];
    updateLeaf(book, turn(1, 0, "anim"), leafV, foot);
    expect(tip(NV >> 1)).toBeGreaterThan(0.5);
    updateLeaf(book, turn(1, 1, "anim"), leafV, foot);
    expect(tip(NV >> 1)).toBeLessThan(-0.5);
  });

  it("indexes every leaf vertex", () => {
    const idx = leafIndices();
    expect(Math.max(...idx)).toBe(NU * NV - 1);
  });
});
