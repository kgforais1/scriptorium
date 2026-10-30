import { describe, expect, it } from "vitest";
import { bookIdHint, isValidBookId } from "./bookId";

// Rejected cases from src-tauri/src/book_format/tests.rs and library/slug.rs.
const REJECTED = [
  "",
  "../etc",
  "Has Caps",
  ".",
  "..",
  "bad/id",
  "con",
  "aux",
  "nul",
  "prn",
  "com1",
  "com9",
  "lpt9",
  "COM9",
  "con.txt",
  "prn.png",
  "book.",
  "CON.",
  "ok\n",
  "-",
  "_x",
  ".hidden",
  "  ",
  String.raw`a/b\c`,
  "nul\0",
  "日本語",
  "x".repeat(65),
];

// Accepted cases from src-tauri/src/library/slug.rs, plus near-misses of the reserved names.
const ACCEPTED = [
  "a",
  "0",
  "ok",
  "a.b_c-d",
  "book-1",
  "v1.2",
  "my-book",
  "x".repeat(64),
  "com",
  "com10",
  "con-x",
  "conx",
  "a.con",
];

describe("isValidBookId", () => {
  it.each(REJECTED)("rejects %j", (id) => {
    expect(isValidBookId(id)).toBe(false);
  });

  it.each(ACCEPTED)("accepts %j", (id) => {
    expect(isValidBookId(id)).toBe(true);
  });
});

describe("bookIdHint", () => {
  it("is null for a valid id", () => {
    expect(bookIdHint("my-book")).toBeNull();
  });
  it.each([
    ["", "Enter a book id."],
    ["a".repeat(65), "Use at most 64 characters."],
    ["book.", "Don't end the id with a dot."],
    ["con", "That name is reserved on Windows. Add a word, such as con-book."],
    ["-x", "Start with a lowercase letter or digit."],
    ["Has Caps", "Start with a lowercase letter or digit."],
    ["has caps", "Use only lowercase letters, digits, dots, underscores, and hyphens."],
  ])("names the rule %j breaks", (id, hint) => {
    expect(bookIdHint(id)).toBe(hint);
  });
});
