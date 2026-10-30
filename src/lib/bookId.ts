const MAX_LEN = 64;

// Keep in step with WINDOWS_RESERVED_STEMS in src-tauri/src/book_format/mod.rs.
const WINDOWS_RESERVED = new Set([
  "con",
  "prn",
  "aux",
  "nul",
  ...Array.from({ length: 9 }, (_, i) => `com${i + 1}`),
  ...Array.from({ length: 9 }, (_, i) => `lpt${i + 1}`),
]);

function isLowerAlnum(ch: string): boolean {
  return (ch >= "a" && ch <= "z") || (ch >= "0" && ch <= "9");
}

function isReservedName(id: string): boolean {
  const stem = id.split(".")[0].toLowerCase();
  return WINDOWS_RESERVED.has(stem);
}

/**
 * Tells whether a book id is acceptable. It mirrors `validate_book_id` in Rust.
 *
 * @param id - The candidate id.
 * @returns True for 1 to 64 characters of lowercase letters, digits, `.`, `_`, and `-`, that
 * start with a letter or digit, do not end with a period, and are not a Windows device name.
 */
export function isValidBookId(id: string): boolean {
  if (id.length === 0 || id.length > MAX_LEN || id.endsWith(".")) {
    return false;
  }
  if (isReservedName(id) || !isLowerAlnum(id[0])) {
    return false;
  }
  return [...id].every((ch) => isLowerAlnum(ch) || ch === "." || ch === "_" || ch === "-");
}

/**
 * Explains why a book id is not acceptable, in a short hint for the form.
 *
 * @param id - The candidate id.
 * @returns A hint, or null when `isValidBookId(id)` is true.
 */
export function bookIdHint(id: string): string | null {
  if (isValidBookId(id)) {
    return null;
  }
  if (id.length === 0) {
    return "Enter a book id.";
  }
  if (id.length > MAX_LEN) {
    return `Use at most ${MAX_LEN} characters.`;
  }
  if (id.endsWith(".")) {
    return "Don't end the id with a dot.";
  }
  if (isReservedName(id)) {
    return "That name is reserved on Windows. Add a word, such as con-book.";
  }
  if (!isLowerAlnum(id[0])) {
    return "Start with a lowercase letter or digit.";
  }
  return "Use only lowercase letters, digits, dots, underscores, and hyphens.";
}
