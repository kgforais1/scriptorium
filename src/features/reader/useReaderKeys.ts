import { useEffect, type RefObject } from "react";
import { applyKeyAction, keyAction, shouldIgnoreKey, type Pager } from "./keys";

const dialogOpen = () => document.querySelector("dialog[open]") !== null;

/**
 * Turns pages from the keyboard with a window-level handler, so the keys work without focusing
 * the canvas. Arrows and Page Up/Down turn, Home and End jump. Fields (the scrubber included),
 * held modifiers, and open dialogs are left alone.
 *
 * @param controllerRef - The engine controller, or null while it starts.
 * @param spreads - How many spreads the book has.
 */
export function usePageKeys(controllerRef: RefObject<Pager | null>, spreads: number): void {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const action = keyAction(event.key);
      const controller = controllerRef.current;
      if (!action || !controller || shouldIgnoreKey(event, dialogOpen())) return;
      event.preventDefault();
      applyKeyAction(action, controller, spreads);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [controllerRef, spreads]);
}

/** Calls `onEscape` when Escape is pressed while `active`, unless a dialog is open on top. */
export function useEscape(active: boolean, onEscape: () => void): void {
  useEffect(() => {
    if (!active) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape" && !dialogOpen()) onEscape();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [active, onEscape]);
}
