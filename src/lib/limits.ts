// Keep in step with TITLE_MAX_LEN and TEXT_FIELD_MAX_LEN in src-tauri/src/book_format/mod.rs.
/** Longest title `meta.json` accepts, in Unicode characters. */
export const TITLE_MAX_LEN = 4096;
/** Longest rights or attribution text `meta.json` accepts, in Unicode characters. */
export const TEXT_FIELD_MAX_LEN = 8192;

/**
 * Checks a text field against its length limit, counted the way Rust counts `chars()`.
 *
 * @param value - The field text.
 * @param max - The limit in Unicode characters.
 * @returns A short hint when the text is too long, or null when it fits.
 */
export function lengthHint(value: string, max: number): string | null {
  const length = [...value].length;
  return length > max ? `Use at most ${max} characters (now ${length}).` : null;
}
