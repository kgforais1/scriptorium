import { describe, expect, it } from "vitest";
import { COLOR_FUNCTION, HEX_COLOR, findSkinViolations, isCheckedFile } from "./skinLiterals";

const rules = (path: string, text: string) => findSkinViolations(path, text).map((v) => `${v.line}:${v.rule}`);

describe("HEX_COLOR", () => {
  it.each(["#fff", "#ffff", "#d9b36e", "#d9b36e1c", "#ABCDEF"])("matches %s", (s) => {
    expect(HEX_COLOR.test(`x ${s};`)).toBe(true);
  });

  it.each(["#spike", "#12345", "#fffffffff", "#fff-bar", "#"])("does not match %s", (s) => {
    expect(HEX_COLOR.test(`x ${s};`)).toBe(false);
  });
});

describe("COLOR_FUNCTION", () => {
  it.each(["rgb(0 0 0)", "rgba(0,0,0,.5)", "hsl(10 10% 10%)", "HSLA(1,1%,1%,1)", "oklch(0.5 0.1 90)"])("matches %s", (s) => {
    expect(COLOR_FUNCTION.test(s)).toBe(true);
  });

  it("ignores identifiers that merely end in rgb", () => {
    expect(COLOR_FUNCTION.test("toRgb(x)")).toBe(false);
  });
});

describe("findSkinViolations in CSS", () => {
  it("accepts var(), pixel sizes, zero radius, and keyword colours", () => {
    const css = `.a {
  color: var(--sk-color-ink);
  background: transparent;
  border: 1px solid var(--sk-color-hairline);
  border-radius: 0;
  border-radius: 0 0 var(--sk-radius-bar) var(--sk-radius-bar);
  font-family: var(--sk-font-sans);
  font-family: inherit;
  backdrop-filter: blur(var(--sk-blur-bar));
  filter: none;
  min-height: 44px;
}`;
    expect(rules("a.css", css)).toEqual([]);
  });

  it("flags hex and colour functions with their line", () => {
    expect(rules("a.css", ".a {\n  color: #fff;\n  background: rgba(0, 0, 0, 0.5);\n}")).toEqual([
      "2:hex-color",
      "3:color-function",
    ]);
  });

  it("flags named colours as values, not as parts of other words or variables", () => {
    expect(rules("a.css", ".a { color: red; }")).toEqual(["1:named-color"]);
    expect(rules("a.css", ".a { border: 1px solid white; }")).toEqual(["1:named-color"]);
    expect(rules("a.css", ".red { color: var(--sk-red); animation-name: reddish; }")).toEqual([]);
  });

  it("flags font-family that is not a --sk-* variable", () => {
    expect(rules("a.css", ".a { font-family: Georgia, serif; }")).toEqual(["1:font-family-literal"]);
    expect(rules("a.css", ".a { font-family: var(--other); }")).toEqual(["1:font-family-literal"]);
  });

  it("flags a font-family whose value wraps onto a second line", () => {
    expect(rules("a.css", ".a {\n  font-family:\n    Georgia,\n    serif;\n}")).toEqual(["2:font-family-literal"]);
  });

  it("flags radius literals other than 0 and var()", () => {
    expect(rules("a.css", ".a { border-radius: 10px; }")).toEqual(["1:radius-literal"]);
    expect(rules("a.css", ".a { border-top-left-radius: 50%; }")).toEqual(["1:radius-literal"]);
    expect(rules("a.css", ".a { border-radius: 0 4px 0 0; }")).toEqual(["1:radius-literal"]);
  });

  it("flags filter and backdrop-filter calls without a var()", () => {
    expect(rules("a.css", ".a { backdrop-filter: blur(22px); }")).toEqual(["1:filter-literal"]);
    expect(rules("a.css", ".a { -webkit-backdrop-filter: blur(var(--sk-blur-bar)) saturate(1.4); }")).toEqual([
      "1:filter-literal",
    ]);
    expect(rules("a.css", ".a { filter: drop-shadow(0 0 4px var(--sk-color-ink)); }")).toEqual([]);
  });

  it("ignores comments but keeps line numbers", () => {
    expect(rules("a.css", "/* color: #fff;\n   red */\n.a { color: #000; }")).toEqual(["3:hex-color"]);
  });

  it("ignores selectors that look like declarations", () => {
    expect(rules("a.css", "a:hover { color: var(--sk-color-ink); }\n.b:not(.red) { margin: 0; }")).toEqual([]);
  });
});

describe("findSkinViolations in TS and TSX", () => {
  it("flags literals in style objects", () => {
    const tsx = `const s = {
  color: "red",
  background: '#123456',
  fontFamily: "Georgia, serif",
  borderRadius: 8,
  backdropFilter: "blur(10px)",
};`;
    expect(rules("a.tsx", tsx)).toEqual([
      "2:named-color",
      "3:hex-color",
      "4:font-family-literal",
      "5:radius-literal",
      "6:filter-literal",
    ]);
  });

  it("accepts var() strings and zero radius", () => {
    const tsx = `const s = { color: "var(--sk-color-ink)", borderRadius: 0, fontFamily: "var(--sk-font-sans)", width: 44 };`;
    expect(rules("a.tsx", tsx)).toEqual([]);
  });

  it("ignores // and block comments", () => {
    expect(rules("a.ts", "// color: #fff\nconst x = 1; // rgb(1,2,3)\n/* #000 */\nconst h = '#spike';")).toEqual([]);
  });

  it("does not treat a URL as a comment", () => {
    expect(rules("a.ts", 'const u = "https://example.com/#fff";')).toEqual(["1:hex-color"]);
  });

  it("does not flag a type annotation", () => {
    expect(rules("a.ts", "type P = { color: string; borderRadius: number };")).toEqual([]);
  });
});

describe("isCheckedFile", () => {
  it.each([
    ["src/features/reader/Chrome.tsx", true],
    ["src/features/reader/reader.css", true],
    ["src/features/reader/useThing.ts", true],
    ["src/features/reader/engine/shaders.ts", false],
    ["src/features/reader/engine/x/y.css", false],
    ["src/features/reader/useThing.test.ts", false],
    ["src/features/reader/Chrome.test.tsx", false],
    ["src/features/reader/scene.json", false],
    [String.raw`src\features\reader\engine\a.ts`, false],
  ])("%s -> %s", (path, expected) => {
    expect(isCheckedFile(path)).toBe(expected);
  });
});
