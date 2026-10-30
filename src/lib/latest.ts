/**
 * Wraps an async `send` so at most one call is in flight. A value pushed while a call runs
 * replaces any earlier waiting value and is sent when the call ends. So results arrive in
 * order, and the last value pushed is always the last one sent.
 *
 * @param send - Sends one value, such as a page to save.
 * @returns A function that queues a value.
 */
export function latestOnly<T>(send: (value: T) => Promise<void>): (value: T) => void {
  let running = false;
  let waiting: { value: T } | null = null;

  async function drain(): Promise<void> {
    running = true;
    while (waiting !== null) {
      const { value } = waiting;
      waiting = null;
      await send(value).catch(() => undefined);
    }
    running = false;
  }

  return (value: T) => {
    waiting = { value };
    if (!running) {
      void drain();
    }
  };
}
