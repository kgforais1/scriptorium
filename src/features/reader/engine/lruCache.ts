// A small least-recently-used cache for GPU textures. It knows nothing about WebGL: the owner
// supplies a drop callback, so the eviction rules are unit tested with plain values.

/** Cache of values by integer key. The newest entry and any pinned keys are never evicted. */
export class LruCache<T> {
  private readonly entries = new Map<number, { value: T; used: number }>();
  private tick = 0;

  /** `capacity` is the entry count above which the least recently used unpinned entries drop. */
  constructor(
    private readonly capacity: number,
    private readonly onDrop: (value: T) => void,
  ) {}

  /** The number of cached entries. */
  get size(): number {
    return this.entries.size;
  }

  /** Whether the key is cached. Does not count as a use. */
  has(key: number): boolean {
    return this.entries.has(key);
  }

  /** The cached value, marked as used, or undefined. */
  get(key: number): T | undefined {
    const e = this.entries.get(key);
    if (!e) return undefined;
    e.used = ++this.tick;
    return e.value;
  }

  /**
   * Stores a value, dropping a previous value for the same key, then evicts down to capacity.
   * The stored key and every pinned key survive.
   */
  set(key: number, value: T, pinned: Iterable<number> = []): void {
    const old = this.entries.get(key);
    if (old) this.onDrop(old.value);
    this.entries.set(key, { value, used: ++this.tick });
    this.evict(new Set([key, ...pinned]));
  }

  /** A snapshot of the entries. Does not count as a use. */
  snapshot(): [number, T][] {
    return [...this.entries].map(([k, e]) => [k, e.value]);
  }

  /** Drops every entry. */
  clear(): void {
    for (const e of this.entries.values()) this.onDrop(e.value);
    this.entries.clear();
  }

  private evict(keep: Set<number>): void {
    while (this.entries.size > this.capacity) {
      let oldest: number | null = null;
      let oldestUsed = Infinity;
      for (const [k, e] of this.entries) {
        if (!keep.has(k) && e.used < oldestUsed) {
          oldest = k;
          oldestUsed = e.used;
        }
      }
      if (oldest === null) return;
      this.onDrop(this.entries.get(oldest)!.value);
      this.entries.delete(oldest);
    }
  }
}
