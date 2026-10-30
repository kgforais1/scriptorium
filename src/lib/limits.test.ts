import { describe, expect, it } from "vitest";
import { lengthHint, TEXT_FIELD_MAX_LEN, TITLE_MAX_LEN } from "./limits";

describe("lengthHint", () => {
  it("accepts text at the limit", () => {
    expect(lengthHint("a".repeat(TITLE_MAX_LEN), TITLE_MAX_LEN)).toBeNull();
  });

  it("names the limit when the text is longer", () => {
    expect(lengthHint("a".repeat(TEXT_FIELD_MAX_LEN + 1), TEXT_FIELD_MAX_LEN)).toBe(
      `Use at most ${TEXT_FIELD_MAX_LEN} characters (now ${TEXT_FIELD_MAX_LEN + 1}).`,
    );
  });

  it("counts characters, not UTF-16 units, like Rust", () => {
    expect(lengthHint("😀".repeat(3), 3)).toBeNull();
    expect(lengthHint("😀".repeat(4), 3)).not.toBeNull();
  });
});
