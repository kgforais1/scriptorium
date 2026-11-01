import { useSyncExternalStore } from "react";

const QUERY = "(prefers-reduced-motion: reduce)";

function subscribe(notify: () => void): () => void {
  const list = window.matchMedia(QUERY);
  list.addEventListener("change", notify);
  return () => list.removeEventListener("change", notify);
}

/** Whether the user asked the system for reduced motion. Updates when the preference changes. */
export function useReducedMotion(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => window.matchMedia(QUERY).matches,
    () => false,
  );
}
