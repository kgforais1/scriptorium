import { describe, expect, it } from "vitest";
import { buildSkinCss, checkRequired, flattenTokens, propertyName, tokenToCss } from "./skinTokens";

describe("tokenToCss", () => {
  it("writes a color as given", () => {
    expect(tokenToCss("c", "color", "#d9b36e1c")).toBe("#d9b36e1c");
  });

  it("joins a dimension object and a duration object with their unit", () => {
    expect(tokenToCss("r", "dimension", { value: 10, unit: "px" })).toBe("10px");
    expect(tokenToCss("d", "duration", { value: 650, unit: "ms" })).toBe("650ms");
  });

  it("accepts a plain string for a dimension or duration", () => {
    expect(tokenToCss("r", "dimension", "0.5rem")).toBe("0.5rem");
    expect(tokenToCss("d", "duration", "1s")).toBe("1s");
  });

  it("writes a number as written", () => {
    expect(tokenToCss("s", "number", 1.25)).toBe("1.25");
    expect(tokenToCss("s", "number", 0)).toBe("0");
  });

  it("writes a cubicBezier as cubic-bezier()", () => {
    expect(tokenToCss("e", "cubicBezier", [0.2, 0.75, 0.15, 1])).toBe("cubic-bezier(0.2, 0.75, 0.15, 1)");
  });

  it("quotes font names and leaves generic and system keywords bare", () => {
    expect(tokenToCss("f", "fontFamily", ["-apple-system", "SF Pro Text", "Georgia", "serif", "system-ui"])).toBe(
      '-apple-system, "SF Pro Text", "Georgia", serif, system-ui',
    );
  });

  it("accepts a single font name as a string and escapes quotes", () => {
    expect(tokenToCss("f", "fontFamily", 'My "Font"')).toBe(String.raw`"My \"Font\""`);
  });

  it("rejects an unknown $type, naming the path", () => {
    expect(() => tokenToCss("a.b", "gradient", "x")).toThrow('a.b: unknown $type "gradient"');
  });

  it.each([
    ["color", 5],
    ["color", ""],
    ["dimension", { value: 1, unit: "furlong" }],
    ["dimension", { value: "1", unit: "px" }],
    ["duration", { value: 1, unit: "px" }],
    ["duration", 7],
    ["number", "1.2"],
    ["number", Number.NaN],
    ["cubicBezier", [1, 2, 3]],
    ["cubicBezier", [1, 2, 3, "x"]],
    ["fontFamily", []],
    ["fontFamily", [1]],
  ])("rejects a malformed %s value", (type, value) => {
    expect(() => tokenToCss("x", type, value)).toThrow(/^x:/);
  });
});

describe("flattenTokens", () => {
  it("inherits $type down the tree and lets a leaf override it", () => {
    const tokens = flattenTokens({
      color: { $type: "color", a: { $value: "#fff" }, deep: { b: { $value: "#000" } } },
      motion: { fade: { $type: "duration", $value: { value: 1, unit: "s" } } },
      mixed: { $type: "color", size: { $type: "dimension", $value: "4px" } },
    });
    expect(tokens.map((t) => [t.path, t.type])).toEqual([
      ["color.a", "color"],
      ["color.deep.b", "color"],
      ["motion.fade", "duration"],
      ["mixed.size", "dimension"],
    ]);
  });

  it("ignores $-prefixed metadata at any level", () => {
    const tokens = flattenTokens({
      $description: "root",
      g: { $type: "color", $description: "group", t: { $value: "#fff", $description: "leaf" } },
    });
    expect(tokens).toHaveLength(1);
  });

  it("keeps file order", () => {
    const tokens = flattenTokens({ g: { $type: "color", z: { $value: "1" }, a: { $value: "2" } } });
    expect(tokens.map((t) => t.path)).toEqual(["g.z", "g.a"]);
  });

  it("rejects a token with no type", () => {
    expect(() => flattenTokens({ g: { t: { $value: "1" } } })).toThrow("g.t: token has no $type");
  });

  it("rejects a $type that is not a string, at any level", () => {
    expect(() => flattenTokens({ g: { $type: 5, t: { $value: "1" } } })).toThrow("g: $type must be a string");
    expect(() => flattenTokens({ g: { t: { $type: ["color"], $value: "1" } } })).toThrow("g.t: $type must be a string");
    expect(() => flattenTokens({ $type: null, g: { t: { $value: "1" } } })).toThrow("tokens file: $type must be a string");
  });

  it("inherits a root-level $type", () => {
    expect(flattenTokens({ $type: "color", g: { t: { $value: "#fff" } } })[0].type).toBe("color");
  });

  it("rejects non-object nodes", () => {
    expect(() => flattenTokens([])).toThrow("expected a JSON object");
    expect(() => flattenTokens({ g: 3 })).toThrow("g: expected a token or a group");
    expect(() => flattenTokens({ g: { $type: "color", t: "#fff" } })).toThrow("g.t: expected a token or a group");
  });
});

describe("propertyName", () => {
  it("joins the path with dashes under --sk-", () => {
    expect(propertyName("color.ink-muted")).toBe("--sk-color-ink-muted");
    expect(propertyName("motion.ease")).toBe("--sk-motion-ease");
  });
});

describe("checkRequired", () => {
  const tok = (path: string, type = "color") => ({ path, type });

  it("passes when the sets match, in any order", () => {
    expect(checkRequired([tok("a"), tok("b")], [tok("b"), tok("a")])).toEqual([]);
  });

  it("names a missing required token", () => {
    expect(checkRequired([tok("a")], [tok("a"), tok("b")])).toEqual(["b: required token is missing"]);
  });

  it("names an unexpected token", () => {
    expect(checkRequired([tok("a"), tok("x")], [tok("a")])).toEqual(["x: unexpected token (not in required-tokens.json)"]);
  });

  it("names a token of the wrong type", () => {
    expect(checkRequired([tok("a", "number")], [tok("a", "color")])).toEqual(["a: expected $type color, got number"]);
  });

  it("reports a path defined twice, such as a dotted key next to a nested group", () => {
    const tokens = flattenTokens({
      "color.ink": { $type: "color", $value: "#000" },
      color: { $type: "color", ink: { $value: "#111" } },
    });
    expect(checkRequired(tokens, [tok("color.ink")])).toEqual(["color.ink: defined more than once"]);
  });
});

describe("buildSkinCss", () => {
  it("emits a header, a :root block, and one property per token", () => {
    const css = buildSkinCss("demo", {
      color: { $type: "color", ink: { $value: "#fff" } },
      radius: { $type: "dimension", bar: { $value: { value: 16, unit: "px" } } },
    });
    expect(css).toBe(
      [
        "/* Generated from design/skins/demo/tokens.json by scripts/build-skin-css.mjs. Do not edit. */",
        ":root {",
        "  --sk-color-ink: #fff;",
        "  --sk-radius-bar: 16px;",
        "}",
        "",
      ].join("\n"),
    );
  });

  it("is deterministic", () => {
    const tree = { c: { $type: "color", a: { $value: "#111" }, b: { $value: "#222" } } };
    expect(buildSkinCss("x", tree)).toBe(buildSkinCss("x", tree));
  });
});
