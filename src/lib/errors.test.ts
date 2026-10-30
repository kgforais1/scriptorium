import { describe, expect, it } from "vitest";
import { errorHeading, isCommandError, toCommandError } from "./errors";

const KINDS = [
  "EmptyFolder",
  "NoSupportedImages",
  "UnreadableFile",
  "DestinationExists",
  "DuplicatePageStem",
  "InsufficientDiskSpace",
  "UnsupportedFormatVersion",
  "InvalidPackage",
  "ChecksumMismatch",
  "PermissionDenied",
  "Internal",
];

describe("errorHeading", () => {
  it.each(KINDS)("has a heading for %s", (kind) => {
    expect(errorHeading(kind).length).toBeGreaterThan(0);
  });

  it("names a damaged book", () => {
    expect(errorHeading("ChecksumMismatch")).toBe("Book is damaged");
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
