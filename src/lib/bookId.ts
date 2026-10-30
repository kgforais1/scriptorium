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
