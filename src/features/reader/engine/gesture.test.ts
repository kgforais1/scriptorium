import { describe, expect, it } from "vitest";
import { computeCamera } from "./camera";
import { cornerAt, dragProgress, pickLocal, turnCompletes, type Drag } from "./gesture";
import { makeBook, type Turn } from "./geometry";

const drag = (vx: number): Drag => ({ id: 1, sx: 0, sy: 0, x0: 0.5, lx: 0.5, lt: 0, vx, moved: true });
const turn = (dir: 1 | -1, t: number) => ({ dir, t }) as Turn;

describe("dragProgress", () => {
  it("is 0 at the grab point and 1 at the far side, forward", () => {
    expect(dragProgress(1, 0.5, 0.5)).toBe(0);
    expect(dragProgress(1, 0.5, -0.95)).toBe(1);
    expect(dragProgress(1, 0.5, 5)).toBe(0);
    expect(dragProgress(1, 0.5, -9)).toBe(1);
  });

  it("runs from 1 down to 0 backward", () => {
    expect(dragProgress(-1, -0.5, -0.5)).toBe(1);
    expect(dragProgress(-1, -0.5, 0.95)).toBe(0);
  });
});

describe("turnCompletes", () => {
  it("completes a forward drag past the midpoint", () => {
    expect(turnCompletes(turn(1, 0.6), drag(0), true)).toBe(true);
    expect(turnCompletes(turn(1, 0.3), drag(0), true)).toBe(false);
  });

  it("completes a short forward drag on a flick toward the far side", () => {
    expect(turnCompletes(turn(1, 0.2), drag(-2), true)).toBe(true);
  });

  it("refuses to complete when flicked back", () => {
    expect(turnCompletes(turn(1, 0.7), drag(1.5), true)).toBe(false);
  });

  it("completes a backward drag past the midpoint or on a flick", () => {
    expect(turnCompletes(turn(-1, 0.3), drag(0), true)).toBe(true);
    expect(turnCompletes(turn(-1, 0.8), drag(2), true)).toBe(true);
    expect(turnCompletes(turn(-1, 0.8), drag(0), true)).toBe(false);
  });

  it("settles a cancelled drag to the nearer side, ignoring velocity", () => {
    expect(turnCompletes(turn(1, 0.6), drag(5), false)).toBe(true);
    expect(turnCompletes(turn(1, 0.4), drag(-5), false)).toBe(false);
    expect(turnCompletes(turn(-1, 0.4), drag(-5), false)).toBe(true);
    expect(turnCompletes(turn(-1, 0.6), drag(5), false)).toBe(false);
  });
});

describe("cornerAt", () => {
  const book = makeBook(1.4, 10);

  it("finds the outer corners of both pages", () => {
    expect(cornerAt(book, [0.95, 0.65])).toEqual({ side: 1, cv: 0 });
    expect(cornerAt(book, [-0.95, -0.65])).toEqual({ side: -1, cv: 1 });
  });

  it("ignores the middle of the page and the gutter", () => {
    expect(cornerAt(book, [0.5, 0])).toBeNull();
    expect(cornerAt(book, [0.02, 0.65])).toBeNull();
  });
});

describe("pickLocal", () => {
  it("hits the book centre at the canvas centre", () => {
    const cam = computeCamera(1000, 800, makeBook(1.4, 10), 18);
    const [x] = pickLocal(cam.inv, 1000, 800, 500, 24 + (800 - 24 - 88) / 2);
    expect(Math.abs(x)).toBeLessThan(0.01);
  });
});
