import { describe, expect, it } from "vitest";
import { errorHeading, isCommandError, toCommandError } from "./errors";

// Every BookError variant plus Internal, pinned by value so a wrong or duplicated heading fails.
const HEADINGS: [string, string][] = [
  ["EmptyFolder", "Folder is empty"],
  ["NoSupportedImages", "No supported images"],
  ["UnreadableFile", "Can't read a file"],
  ["DestinationExists", "Book already exists"],
  ["DuplicatePageStem", "Two files share a name"],
  ["InsufficientDiskSpace", "Not enough disk space"],
  ["UnsupportedFormatVersion", "Book is from a newer version"],
  ["InvalidPackage", "Not a valid book"],
  ["ChecksumMismatch", "Book is damaged"],
  ["PermissionDenied", "Access denied"],
  ["Internal", "Scriptorium hit a problem"],
];

describe("errorHeading", () => {
  it.each(HEADINGS)("names %s as %s", (kind, heading) => {
    expect(errorHeading(kind)).toBe(heading);
  });

  it("gives every kind its own heading, distinct from the fallback", () => {
    const headings = HEADINGS.map(([, h]) => h);
    expect(new Set(headings).size).toBe(headings.length);
    expect(headings).not.toContain(errorHeading("Nope"));
  });

  it("falls back for unknown kinds, including object prototype names", () => {
    expect(errorHeading("Nope")).toBe("Something went wrong");
    expect(errorHeading("toString")).toBe("Something went wrong");
    expect(errorHeading("")).toBe("Something went wrong");
  });
});

describe("isCommandError", () => {
  it("accepts { kind, message }", () => {
    expect(isCommandError({ kind: "Internal", message: "x" })).toBe(true);
  });

  it.each([null, undefined, "x", 3, {}, { kind: "a" }, { kind: 1, message: "m" }, { message: "m" }])(
    "rejects %j",
    (value) => {
      expect(isCommandError(value)).toBe(false);
    },
  );
});

describe("toCommandError", () => {
  it("passes a CommandError through", () => {
    const err = { kind: "EmptyFolder", message: "empty" };
    expect(toCommandError(err)).toBe(err);
  });

  it("wraps a string", () => {
    expect(toCommandError("boom")).toEqual({ kind: "Unknown", message: "boom" });
  });

  it("wraps an Error", () => {
    expect(toCommandError(new Error("bad"))).toEqual({ kind: "Unknown", message: "Error: bad" });
  });

  it("wraps null and odd objects", () => {
    expect(toCommandError(null)).toEqual({ kind: "Unknown", message: "null" });
    expect(toCommandError({ kind: "x" }).kind).toBe("Unknown");
  });
});
