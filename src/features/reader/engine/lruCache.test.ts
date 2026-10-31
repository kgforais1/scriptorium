import { describe, expect, it } from "vitest";
import { LruCache } from "./lruCache";

function make(capacity: number) {
  const dropped: string[] = [];
  return { cache: new LruCache<string>(capacity, (v) => dropped.push(v)), dropped };
}

describe("LruCache", () => {
  it("evicts the least recently used entry above capacity", () => {
    const { cache, dropped } = make(2);
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");
    expect(dropped).toEqual(["a"]);
    expect(cache.has(1)).toBe(false);
    expect(cache.size).toBe(2);
  });

  it("counts get as a use", () => {
    const { cache, dropped } = make(2);
    cache.set(1, "a");
    cache.set(2, "b");
    expect(cache.get(1)).toBe("a");
    cache.set(3, "c");
    expect(dropped).toEqual(["b"]);
  });

  it("does not count has as a use", () => {
    const { cache, dropped } = make(2);
    cache.set(1, "a");
    cache.set(2, "b");
    cache.has(1);
    cache.set(3, "c");
    expect(dropped).toEqual(["a"]);
  });

  it("never evicts pinned keys, even when over capacity", () => {
    const { cache, dropped } = make(2);
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c", [1, 2]);
    expect(dropped).toEqual([]);
    expect(cache.size).toBe(3);
    cache.set(4, "d", [1, 2]);
    expect(dropped).toEqual(["c"]);
  });

  it("never evicts the key it just stored", () => {
    const { cache } = make(1);
    cache.set(1, "a");
    cache.set(2, "b");
    expect(cache.has(2)).toBe(true);
    expect(cache.has(1)).toBe(false);
  });

  it("drops the previous value when a key is replaced", () => {
    const { cache, dropped } = make(3);
    cache.set(1, "a");
    expect(cache.get(1)).toBe("a");
    cache.set(1, "a2");
    expect(dropped).toEqual(["a"]);
    expect(cache.get(1)).toBe("a2");
    expect(cache.size).toBe(1);
  });

  it("returns undefined for a missing key", () => {
    expect(make(1).cache.get(9)).toBeUndefined();
  });

  it("clear drops everything", () => {
    const { cache, dropped } = make(3);
    cache.set(1, "a");
    cache.set(2, "b");
    cache.clear();
    expect([...dropped].sort((x, y) => x.localeCompare(y))).toEqual(["a", "b"]);
    expect(cache.size).toBe(0);
  });
});
