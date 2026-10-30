const TYPING_TAGS = new Set(["INPUT", "TEXTAREA", "SELECT"]);

/**
 * Tells whether a key event came from a control where the user is typing, so shortcut
 * keys must be left alone.
 *
 * @param target - `KeyboardEvent.target`.
 * @returns True for inputs, textareas, selects, and editable elements.
 */
export function isTypingTarget(target: unknown): boolean {
  if (typeof target !== "object" || target === null) {
    return false;
  }
  const { tagName, isContentEditable } = target as {
    tagName?: unknown;
    isContentEditable?: unknown;
  };
  return (
    isContentEditable === true || (typeof tagName === "string" && TYPING_TAGS.has(tagName))
  );
}
