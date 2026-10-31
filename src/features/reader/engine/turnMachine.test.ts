import { describe, expect, it } from "vitest";
import { makeBook } from "./geometry";
import { TurnMachine, type TurnHost } from "./turnMachine";

function setup(nLeaf: number, opts: { reduced?: boolean; start?: number } = {}) {
  const log: string[] = [];
  const host: TurnHost = {
    stacks: (Ln, Rn, lp, rp) => log.push(`stacks ${Ln} ${Rn} ${lp} ${rp}`),
    rest: (s) => log.push(`rest ${s}`),
    changed: () => log.push("changed"),
    settled: () => log.push("settled"),
    fade: (from) => log.push(`fade ${from}`),
  };
  const m = new TurnMachine(makeBook(1.4, nLeaf), nLeaf, host, opts.reduced ?? false, opts.start ?? 0);
  // Runs the clock until nothing moves, like the frame loop does.
  const settle = () => {
    for (let i = 0; i < 400 && m.turn; i++) m.step(0.05);
  };
  return { m, log, settle };
}

describe("TurnMachine", () => {
  it("clamps the start spread into the book", () => {
    expect(setup(3, { start: 99 }).m.spread).toBe(3);
    expect(setup(3, { start: -2 }).m.spread).toBe(0);
  });

  it("turns forward one spread and reports it", () => {
    const { m, settle } = setup(3);
    m.go(1);
    expect(m.turning).toBe(true);
    expect(m.spread).toBe(0);
    settle();
    expect(m.turning).toBe(false);
    expect(m.spread).toBe(1);
  });

  it("turns backward", () => {
    const { m, settle } = setup(3, { start: 2 });
    m.go(-1);
    settle();
    expect(m.spread).toBe(1);
  });

  it("is a no-op past either end", () => {
    const a = setup(2);
    a.m.go(-1);
    expect(a.m.turn).toBeNull();
    const b = setup(2, { start: 2 });
    b.m.go(1);
    expect(b.m.turn).toBeNull();
    expect(b.log).toEqual([]);
  });

  it("queues navigation during a turn and applies it after", () => {
    const { m, settle } = setup(5);
    m.go(1);
    m.go(1);
    expect(m.queued).toBe(1);
    settle();
    expect(m.spread).toBe(2);
    expect(m.queued).toBe(0);
  });

  it("caps the queue at two steps", () => {
    const { m, settle } = setup(8);
    for (let i = 0; i < 6; i++) m.go(1);
    expect(m.queued).toBe(2);
    settle();
    expect(m.spread).toBe(3);
  });

  it("drops the queue when a step would pass the end of the book", () => {
    const { m, settle } = setup(2, { start: 1 });
    m.go(1);
    m.go(1);
    m.go(1);
    settle();
    expect(m.spread).toBe(2);
    expect(m.queued).toBe(0);
    expect(m.turn).toBeNull();
  });

  it("cancels opposite steps in the queue", () => {
    const { m, settle } = setup(5, { start: 2 });
    m.go(1);
    m.go(1);
    m.go(-1);
    settle();
    expect(m.spread).toBe(4 - 1);
  });

  it("drops navigation during an active drag", () => {
    const { m } = setup(5);
    m.beginTurn(1, "drag", 0.5);
    m.dragging = true;
    m.go(1);
    m.goTo(4);
    expect(m.queued).toBe(0);
    expect(m.turn?.kind).toBe("drag");
  });

  it("completes a corner peek as a turn when navigation matches its side", () => {
    const { m, settle } = setup(5);
    const T = m.beginTurn(1, "peek", 0.9)!;
    expect(m.turning).toBe(false);
    m.go(1);
    expect(T.kind).toBe("anim");
    settle();
    expect(m.spread).toBe(1);
  });

  it("settles a peek of the other side, then applies the queued step", () => {
    const { m, settle } = setup(5, { start: 2 });
    const T = m.beginTurn(1, "peek", 0.9)!;
    m.go(-1);
    expect(T.pkT).toBe(0);
    settle();
    expect(m.spread).toBe(1);
  });

  describe("goTo", () => {
    it("jumps next to a distant spread and turns the last step", () => {
      const { m, log, settle } = setup(10);
      m.goTo(6);
      expect(log).toContain("rest 5");
      expect(m.spread).toBe(5);
      settle();
      expect(m.spread).toBe(6);
    });

    it("turns directly to the next spread", () => {
      const { m, log, settle } = setup(10);
      m.goTo(1);
      expect(log).not.toContain("rest 0");
      settle();
      expect(m.spread).toBe(1);
    });

    it("clamps and ignores the current spread", () => {
      const { m, settle } = setup(3, { start: 1 });
      m.goTo(1);
      expect(m.turn).toBeNull();
      m.goTo(99);
      settle();
      expect(m.spread).toBe(3);
    });

    it("waits for a turn in progress, then goes", () => {
      const { m, settle } = setup(10);
      m.go(1);
      m.goTo(7);
      expect(m.spread).toBe(0);
      settle();
      expect(m.spread).toBe(7);
    });

    it("replaces queued steps", () => {
      const { m, settle } = setup(10);
      m.go(1);
      m.go(1);
      m.goTo(0);
      settle();
      expect(m.spread).toBe(0);
    });
  });

  describe("reduced motion", () => {
    it("changes spread at once with a fade and no leaf", () => {
      const { m, log } = setup(5, { reduced: true });
      m.go(1);
      expect(m.turn).toBeNull();
      expect(m.spread).toBe(1);
      expect(log).toEqual(["rest 1", "fade 0", "settled", "changed"]);
    });

    it("goTo fades straight to the spread", () => {
      const { m, log } = setup(10, { reduced: true });
      m.goTo(7);
      expect(m.spread).toBe(7);
      expect(log).toContain("fade 0");
    });

    it("is a no-op at the ends", () => {
      const { m, log } = setup(2, { reduced: true });
      m.go(-1);
      expect(log).toEqual([]);
    });
  });

  describe("setReducedMotion", () => {
    it("switches later turns to the cross-fade and back to the curl", () => {
      const { m, log, settle } = setup(5);
      m.setReducedMotion(true);
      m.go(1);
      expect(m.turn).toBeNull();
      expect(log).toContain("fade 0");
      m.setReducedMotion(false);
      m.go(1);
      expect(m.turn).not.toBeNull();
      settle();
      expect(m.spread).toBe(2);
    });

    it("settles an open peek at once when reduced motion turns on", () => {
      const { m } = setup(5);
      const T = m.beginTurn(1, "peek", 0.9)!;
      expect(T.pkT).toBe(1);
      m.setReducedMotion(true);
      expect(T.pkT).toBe(0);
    });

    it("does not touch a peek when reduced motion turns off", () => {
      const { m } = setup(5);
      m.setReducedMotion(true);
      const T = m.beginTurn(1, "peek", 0.9)!;
      m.setReducedMotion(false);
      expect(T.pkT).toBe(1);
    });

    it("never curls an open peek under reduced motion: it settles, then fades", () => {
      const { m, log, settle } = setup(5);
      const T = m.beginTurn(1, "peek", 0.9)!;
      m.setReducedMotion(true);
      m.go(1);
      expect(T.kind).toBe("peek");
      expect(T.anim).toBeNull();
      expect(m.queued).toBe(1);
      expect(m.spread).toBe(0);
      settle();
      expect(m.turn).toBeNull();
      expect(m.spread).toBe(1);
      expect(log).toContain("fade 0");
    });

    it("lets a curl already in the air finish, queueing navigation made meanwhile", () => {
      const { m, log, settle } = setup(5);
      m.go(1);
      m.setReducedMotion(true);
      m.go(1);
      expect(m.queued).toBe(1);
      expect(m.spread).toBe(0);
      settle();
      expect(m.spread).toBe(2);
      expect(log).toContain("fade 1");
    });
  });

  describe("reapply", () => {
    it("redraws the rest stacks when nothing turns", () => {
      const { m, log } = setup(5, { start: 2 });
      m.reapply();
      expect(log).toEqual(["rest 2"]);
    });

    it("redraws the turn stacks mid-turn", () => {
      const { m, log } = setup(5, { start: 2 });
      m.go(1);
      log.length = 0;
      m.reapply();
      expect(log).toEqual([`stacks ${2 + 7} ${5 - 2 - 1 + 7} 4 7`]);
    });
  });

  it("exposes the spread count and the leaf's slots", () => {
    const { m } = setup(4, { start: 1 });
    expect(m.spreads).toBe(5);
    expect(m.turnSlots()).toEqual([]);
    m.go(1);
    expect(m.turnSlots()).toEqual([3, 4]);
  });
});
