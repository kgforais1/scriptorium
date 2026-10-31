// The page-turn state machine: which spread is open, the leaf in motion, and queued navigation.
// It owns no GL and no DOM, so the queueing rules are unit tested. The engine supplies a
// `TurnHost` that redraws the page blocks and publishes changes.

import { gutterLift, type Book, type Turn } from "./geometry";
import { clamp, easeInOut, lerp } from "./math";

/** Visual padding leaves under each page block. */
export const EXTRA = 7;

/** What the machine needs from the engine. */
export interface TurnHost {
  /** Draw the page blocks for a turn: left and right leaf counts, and the slots on show. */
  stacks(Ln: number, Rn: number, lp: number, rp: number): void;
  /** Draw the page blocks at rest on a spread. */
  rest(spread: number): void;
  /** The open spread or the turning flag changed. */
  changed(): void;
  /** A turn or peek ended and the book lies still. */
  settled(): void;
  /** Reduced motion: cross-fade from the spread the book just left. */
  fade(fromSpread: number): void;
}

/** Turn progress, queue, and the current spread. */
export class TurnMachine {
  /** The leaf in motion, a corner peek included, or null. */
  turn: Turn | null = null;
  /** True while the pointer holds a page. Navigation is dropped meanwhile. */
  dragging = false;
  private K: number;
  private queue = 0;
  private pending: number | null = null;

  /** Starts at startSpread, clamped into the book. */
  constructor(
    private readonly book: Book,
    private readonly nLeaf: number,
    readonly host: TurnHost,
    private reducedMotion: boolean,
    startSpread: number,
  ) {
    this.K = clamp(Math.round(startSpread), 0, nLeaf);
  }

  /** The open spread. */
  get spread(): number {
    return this.K;
  }

  /** Steps waiting for the current turn to end (negative is backward). */
  get queued(): number {
    return this.queue;
  }

  /** The number of spreads. */
  get spreads(): number {
    return this.nLeaf + 1;
  }

  /**
   * Switches between the curl and the cross-fade. A turn already in the air finishes as a curl,
   * and the change applies from the next one. A corner peek is not a turn, so it settles at once.
   */
  setReducedMotion(reduced: boolean): void {
    this.reducedMotion = reduced;
    if (reduced && this.turn?.kind === "peek") this.turn.pkT = 0;
  }

  /** True while a leaf is turning. A corner peek is not a turn. */
  get turning(): boolean {
    return !!this.turn && this.turn.kind !== "peek";
  }

  /** The slots a turn needs to keep on the GPU: the leaf's two faces. */
  turnSlots(): number[] {
    return this.turn ? [2 * this.turn.leaf + 1, 2 * this.turn.leaf + 2] : [];
  }

  /**
   * Turns one spread. During an animation, or while a corner peek settles, the step is queued
   * (up to two) and applied when the turn ends. During an active drag it is dropped. Under
   * reduced motion an open peek is never turned into a curl: it settles, then the step runs as
   * a cross-fade.
   */
  go(dir: 1 | -1, dur = 1.05): void {
    if (this.dragging) return;
    const T = this.turn;
    if (!T) {
      if (this.reducedMotion) this.jump(this.K + dir);
      else if (this.beginTurn(dir, "anim")) this.animateTo(dir > 0 ? 1 : 0, dur, easeInOut);
    } else if (T.kind === "peek" && T.dir === dir && !this.reducedMotion) {
      T.A0 = 1.3;
      this.animateTo(dir > 0 ? 1 : 0, dur, easeInOut);
    } else {
      if (T.kind === "peek") T.pkT = 0;
      this.queue = clamp(this.queue + dir, -2, 2);
    }
  }

  /**
   * Opens a spread. A distant spread jumps next to the target and turns the last step, or
   * cross-fades under reduced motion. Called mid-turn it waits for the turn to end.
   */
  goTo(spread: number): void {
    const s = clamp(Math.round(spread), 0, this.nLeaf);
    if (this.dragging) return;
    if (this.turn) {
      this.queue = 0;
      this.pending = s;
      if (this.turn.kind === "peek") this.turn.pkT = 0;
      return;
    }
    this.openSpread(s);
  }

  /** Starts a leaf turning, or returns the one already turning. Null at either end of the book. */
  beginTurn(dir: 1 | -1, kind: Turn["kind"], cv?: number): Turn | null {
    if (this.turn) return this.turn;
    const K = this.K;
    if ((dir > 0 && K >= this.nLeaf) || (dir < 0 && K <= 0)) return null;
    const leaf = dir > 0 ? K : K - 1;
    const { Ln, Rn } = this.leafCounts(dir);
    this.host.stacks(Ln, Rn, dir > 0 ? 2 * K : 2 * K - 2, dir > 0 ? 2 * K + 3 : 2 * K + 1);
    const b = this.book;
    this.turn = {
      dir, leaf, t: dir > 0 ? 0 : 1, kind, cv: cv ?? 0.94, A0: 0.55, pk: 0, pkT: 1, lift: 0, cancel: 0,
      gA: dir > 0 ? gutterLift(b, Rn + 1) : gutterLift(b, Ln + 1),
      gB: dir > 0 ? gutterLift(b, Ln + 1) : gutterLift(b, Rn + 1),
      anim: null,
    };
    this.host.changed();
    return this.turn;
  }

  /** Redraws the page blocks for the current state, after the GPU lost them. */
  reapply(): void {
    const T = this.turn;
    if (!T) {
      this.host.rest(this.K);
      return;
    }
    const K = this.K, { Ln, Rn } = this.leafCounts(T.dir);
    this.host.stacks(Ln, Rn, T.dir > 0 ? 2 * K : 2 * K - 2, T.dir > 0 ? 2 * K + 3 : 2 * K + 1);
  }

  // Leaves left on each block while a leaf in direction dir is in the air.
  private leafCounts(dir: 1 | -1): { Ln: number; Rn: number } {
    const K = this.K;
    return {
      Ln: dir > 0 ? K + EXTRA : K - 1 + EXTRA,
      Rn: dir > 0 ? this.nLeaf - K - 1 + EXTRA : this.nLeaf - K + EXTRA,
    };
  }

  /** Animates the open turn to t = 0 or 1. */
  animateTo(target: number, dur: number, ease: (x: number) => number): void {
    const T = this.turn!;
    if (T.kind === "peek") T.lift = T.pk;
    T.kind = "anim";
    const tp = T.dir > 0 ? T.t : 1 - T.t, toStart = T.dir > 0 ? target === 0 : target === 1;
    T.cancel = toStart ? Math.max(tp, 0.001) : 0;
    T.anim = { t0: T.t, t1: target, el: 0, dur, ease };
    this.host.changed();
  }

  /** Advances the turn by dt seconds. Returns true while something moves. */
  step(dt: number): boolean {
    const T = this.turn;
    if (!T) return false;
    return T.kind === "peek" ? this.stepPeek(T, dt) : this.stepTurn(T, dt);
  }

  private stepPeek(T: Turn, dt: number): boolean {
    T.pk += (T.pkT - T.pk) * (1 - Math.exp(-dt * 9));
    if (T.pkT === 0 && T.pk < 0.01) {
      this.turn = null;
      this.host.rest(this.K);
      this.host.changed();
      this.host.settled();
      this.drain();
    }
    return true;
  }

  private stepTurn(T: Turn, dt: number): boolean {
    let moving = T.kind === "drag";
    if (T.lift < 1) {
      T.lift = Math.min(1, T.lift + (1 - T.lift) * (1 - Math.exp(-dt * 12)) + dt * 0.2);
      moving = true;
    }
    const A = T.anim;
    if (A) {
      A.el += dt;
      const k = clamp(A.el / A.dur, 0, 1);
      T.t = lerp(A.t0, A.t1, A.ease(k));
      moving = true;
      if (k >= 1) {
        T.t = A.t1;
        T.anim = null;
        this.finishTurn();
      }
    }
    return moving;
  }

  private finishTurn(): void {
    const T = this.turn!;
    this.turn = null;
    const landedLeft = T.t >= 0.5;
    if (T.dir > 0 && landedLeft) this.K++;
    else if (T.dir < 0 && !landedLeft) this.K--;
    this.host.rest(this.K);
    this.host.settled();
    this.host.changed();
    this.drain();
  }

  private drain(): void {
    if (this.pending !== null) {
      const s = this.pending;
      this.pending = null;
      this.queue = 0;
      this.openSpread(s);
      return;
    }
    if (this.queue === 0) return;
    const d = Math.sign(this.queue) as 1 | -1;
    // A step past either end of the book is dropped, along with the rest of the queue.
    if ((d > 0 && this.K >= this.nLeaf) || (d < 0 && this.K <= 0)) {
      this.queue = 0;
      return;
    }
    this.queue -= d;
    this.go(d, 0.75);
  }

  private openSpread(s: number): void {
    if (s === this.K) return;
    if (this.reducedMotion) {
      this.jump(s);
      return;
    }
    const dir = Math.sign(s - this.K) as 1 | -1;
    if (Math.abs(s - this.K) > 1) {
      this.K = s - dir;
      this.host.rest(this.K);
    }
    this.go(dir, 0.9);
  }

  // Reduced motion: no leaf moves. The book changes spread at once and the host cross-fades.
  private jump(s: number): void {
    const to = clamp(s, 0, this.nLeaf);
    if (to === this.K) return;
    const from = this.K;
    this.K = to;
    this.host.rest(to);
    this.host.fade(from);
    this.host.settled();
    this.host.changed();
  }
}
