import { describe, expect, it } from "vitest";
import { latestOnly } from "./latest";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => (resolve = r));
  return { promise, resolve };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("latestOnly", () => {
  it("sends one at a time and skips values replaced while waiting", async () => {
    const sent: number[] = [];
    const gates: ReturnType<typeof deferred>[] = [];
    const push = latestOnly<number>((v) => {
      sent.push(v);
      const gate = deferred();
      gates.push(gate);
      return gate.promise;
    });
    push(1);
    push(2);
    push(3);
    await tick();
    expect(sent).toEqual([1]);
    gates[0].resolve();
    await tick();
    expect(sent).toEqual([1, 3]);
    gates[1].resolve();
    await tick();
    push(4);
    await tick();
    expect(sent).toEqual([1, 3, 4]);
  });

  it("survives a send that throws synchronously", async () => {
    const sent: number[] = [];
    const push = latestOnly<number>((v) => {
      sent.push(v);
      if (v === 1) {
        throw new Error("sync");
      }
      return Promise.resolve();
    });
    push(1);
    push(2);
    await tick();
    expect(sent).toEqual([1, 2]);
  });

  it("keeps going after a failed send", async () => {
    const sent: number[] = [];
    let fail = true;
    const push = latestOnly<number>(async (v) => {
      sent.push(v);
      if (fail) {
        fail = false;
        throw new Error("disk");
      }
    });
    push(1);
    push(2);
    await tick();
    expect(sent).toEqual([1, 2]);
  });
});
