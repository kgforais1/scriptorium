import { describe, expect, it } from "vitest";
import { computeCamera, INSETS } from "./camera";
import {
  cornerAt, dragProgress, gripAt, hoverOnPage, onPageAt, pastSlop, peekStays, pickLocal, settleFor, tapDirection, TAP_SLOP,
  trackDrag, turnCompletes, type Drag, type Tap,
} from "./gesture";
import { easeInOut, easeOut } from "./math";
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
    const [x] = pickLocal(cam.inv, 1000, 800, 500, INSETS.top + (800 - INSETS.top - INSETS.bot) / 2);
    expect(Math.abs(x)).toBeLessThan(0.01);
  });
});

describe("pastSlop", () => {
  it("allows travel up to the slop and no further, on either axis", () => {
    expect(pastSlop(10, 10, 10 + TAP_SLOP, 10)).toBe(false);
    expect(pastSlop(10, 10, 10 - TAP_SLOP, 10 + TAP_SLOP)).toBe(false);
    expect(pastSlop(10, 10, 10 + TAP_SLOP + 1, 10)).toBe(true);
    expect(pastSlop(10, 10, 10, 10 - TAP_SLOP - 1)).toBe(true);
  });
});

describe("onPageAt and hoverOnPage", () => {
  const book = makeBook(1.4, 10);

  it("accepts a press on either page and rejects the gutter and the desk", () => {
    expect(onPageAt(book, [0.5, 0], 0)).toBe(true);
    expect(onPageAt(book, [-0.5, 0], 0)).toBe(true);
    expect(onPageAt(book, [0.01, 0], 0)).toBe(false);
    expect(onPageAt(book, [1.2, 0], 0)).toBe(false);
  });

  it("lets slack extend past the head and tail only", () => {
    expect(onPageAt(book, [0.5, book.H / 2 + 0.05], 0)).toBe(false);
    expect(onPageAt(book, [0.5, book.H / 2 + 0.05], 0.06)).toBe(true);
    expect(onPageAt(book, [1.2, 0], 0.06)).toBe(false);
  });

  it("uses a slightly tighter zone for the hover cursor", () => {
    expect(hoverOnPage(book, [0.5, 0])).toBe(true);
    expect(hoverOnPage(book, [1.06, 0])).toBe(false);
    expect(hoverOnPage(book, [0.03, 0])).toBe(false);
    expect(hoverOnPage(book, [0.5, book.H / 2])).toBe(false);
  });
});

describe("gripAt", () => {
  const book = makeBook(1.4, 10);

  it("reports the grip height from head (0) to tail (1), clamped", () => {
    expect(gripAt(book, book.H / 2).vn).toBe(0);
    expect(gripAt(book, 0).vn).toBeCloseTo(0.5);
    expect(gripAt(book, -book.H / 2).vn).toBe(1);
    expect(gripAt(book, 9).vn).toBe(0);
    expect(gripAt(book, -9).vn).toBe(1);
  });

  it("lifts the corner hard near the head or tail and gently mid-page", () => {
    expect(gripAt(book, book.H / 2).A0).toBeCloseTo(1.3);
    expect(gripAt(book, -book.H / 2).A0).toBeCloseTo(1.3);
    expect(gripAt(book, 0).A0).toBeCloseTo(0.45);
  });
});

describe("peekStays", () => {
  it("stays open on its own corner with nothing queued", () => {
    expect(peekStays(0, 1, { side: 1 })).toBe(true);
  });

  it("settles when the mouse leaves the corner or moves to the other side", () => {
    expect(peekStays(0, 1, null)).toBe(false);
    expect(peekStays(0, 1, { side: -1 })).toBe(false);
  });

  it("settles once a step is queued, even on its own corner", () => {
    expect(peekStays(1, 1, { side: 1 })).toBe(false);
    expect(peekStays(-2, 1, { side: 1 })).toBe(false);
  });

  it("settles while a jump is pending, even on its own corner", () => {
    expect(peekStays(0, 1, { side: 1 }, true)).toBe(false);
  });
});

describe("trackDrag", () => {
  it("smooths velocity toward the instantaneous speed and advances the sample", () => {
    const d = drag(0);
    d.lx = 0.5;
    d.lt = 1000;
    trackDrag(d, 0.4, 1100);
    // -0.1 units in 100 ms is -1 per second; half of it blends in.
    expect(d.vx).toBeCloseTo(-0.5);
    expect([d.lx, d.lt]).toEqual([0.4, 1100]);
  });

  it("treats a zero time step as 1 ms instead of dividing by zero", () => {
    const d = drag(0);
    d.lx = 0;
    d.lt = 5;
    trackDrag(d, 0.001, 5);
    expect(Number.isFinite(d.vx)).toBe(true);
  });
});

describe("settleFor", () => {
  const moved = (vx: number): Drag => ({ ...drag(vx), moved: true });
  const still = (): Drag => ({ ...drag(0), moved: false });

  it("turns the page when an unmoved press is released, in either direction", () => {
    expect(settleFor(turn(1, 0), still(), "pointerup")).toMatchObject({ target: 1, dur: 1.05, ease: easeInOut });
    expect(settleFor(turn(-1, 1), still(), "pointerup")).toMatchObject({ target: 0, dur: 1.05, ease: easeInOut });
  });

  it("puts an unmoved page back on pointercancel and on lostpointercapture", () => {
    for (const type of ["pointercancel", "lostpointercapture"]) {
      expect(settleFor(turn(1, 0), still(), type)).toMatchObject({ target: 0, dur: 0.3, ease: easeOut });
      expect(settleFor(turn(-1, 1), still(), type)).toMatchObject({ target: 1, dur: 0.3, ease: easeOut });
    }
  });

  it("completes a moved drag past the midpoint and scales the duration by the distance left", () => {
    const s = settleFor(turn(1, 0.6), moved(0), "pointerup");
    expect(s.target).toBe(1);
    expect(s.dur).toBeCloseTo(0.3 + 0.55 * 0.4);
    expect(s.ease).toBe(easeOut);
  });

  it("lets a flick finish a short drag on release but not on a cancel", () => {
    expect(settleFor(turn(1, 0.2), moved(-2), "pointerup").target).toBe(1);
    expect(settleFor(turn(1, 0.2), moved(-2), "pointercancel").target).toBe(0);
    expect(settleFor(turn(1, 0.2), moved(-2), "lostpointercapture").target).toBe(0);
  });

  it("settles a cancelled moved drag to the nearer side, backward as well", () => {
    expect(settleFor(turn(-1, 0.3), moved(0), "pointercancel").target).toBe(0);
    expect(settleFor(turn(-1, 0.7), moved(0), "pointercancel").target).toBe(1);
  });
});

describe("tapDirection", () => {
  const tap: Tap = { id: 1, sx: 100, sy: 100, dir: -1 };

  it("turns on a pointerup inside the slop", () => {
    expect(tapDirection(tap, "pointerup", 102, 99)).toBe(-1);
  });

  it("does not turn after the pointer moved past the slop", () => {
    expect(tapDirection(tap, "pointerup", 110, 100)).toBeNull();
  });

  it("does not turn on a cancel or a lost capture", () => {
    expect(tapDirection(tap, "pointercancel", 100, 100)).toBeNull();
    expect(tapDirection(tap, "lostpointercapture", 100, 100)).toBeNull();
  });
});
