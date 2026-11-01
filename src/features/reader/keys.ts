import { isTypingTarget } from "../../lib/dom";

/** What a reader key press asks for. */
export type KeyAction = "next" | "prev" | "first" | "last";

const KEY_ACTIONS: ReadonlyMap<string, KeyAction> = new Map<string, KeyAction>([
  ["ArrowRight", "next"],
  ["PageDown", "next"],
  ["ArrowLeft", "prev"],
  ["PageUp", "prev"],
  ["Home", "first"],
  ["End", "last"],
]);

/**
 * Maps a `KeyboardEvent.key` to a reader action.
 *
 * @param key - The key value.
 * @returns The action, or null for a key the reader does not use.
 */
export function keyAction(key: string): KeyAction | null {
  return KEY_ACTIONS.get(key) ?? null;
}

/** The parts of a keyboard event that decide whether the reader leaves it alone. */
export type KeyLike = { altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; target: unknown };

/**
 * Whether a key press belongs to something else: a modifier is held, a field has the focus, or a
 * dialog is open.
 *
 * @param event - The key event, or any object with its modifier flags and target.
 * @param dialogOpen - True while a `dialog[open]` is in the document.
 * @returns True when the reader must not act on the key.
 */
export function shouldIgnoreKey(event: KeyLike, dialogOpen: boolean): boolean {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return true;
  return dialogOpen || isTypingTarget(event.target);
}

/** The slice of the engine controller a key press drives. */
export type Pager = { next(): void; prev(): void; goTo(spread: number): void };

/**
 * Runs a key action on the engine. Next and prev queue behind a turn in progress and do nothing
 * at the edges. First and last jump.
 *
 * @param action - What the key asked for.
 * @param pager - The engine controller.
 * @param spreads - How many spreads the book has.
 */
export function applyKeyAction(action: KeyAction, pager: Pager, spreads: number): void {
  if (action === "next") pager.next();
  else if (action === "prev") pager.prev();
  else if (action === "first") pager.goTo(0);
  else pager.goTo(Math.max(0, spreads - 1));
}
