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
  it.each(["rgb(0 0 0)", "rgba(0,0,0,.5)", "hsl(10 10% 10%)", "HSLA(1,1%,1%,1)", "oklch(0.5 0.1 90)", "color(display-p3 1 0 0)"])("matches %s", (s) => {
    expect(COLOR_FUNCTION.test(s)).toBe(true);
  });

  it.each(["color(srgb 1 0 0)", "color(srgb-linear 1 0 0)", "color(xyz-d65 1 0 0)", "color(from red srgb r g b)"])("matches %s", (s) => {
    expect(COLOR_FUNCTION.test(s)).toBe(true);
  });

  it("ignores a plain color(...) call", () => {
    expect(COLOR_FUNCTION.test("color(page)")).toBe(false);
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

describe("review follow-ups", () => {
  it("flags named colours in background-image gradients and svg colour properties", () => {
    expect(rules("a.css", ".a { background-image: linear-gradient(red, var(--sk-color-ink)); }")).toEqual(["1:named-color"]);
    expect(rules("a.css", ".a { background-image: url(red.png); }")).toEqual([]);
    expect(rules("a.css", ".a { stop-color: blue; flood-color: black; }")).toEqual(["1:named-color"]);
  });

  it("flags named colours in JSX and SVG attributes", () => {
    expect(rules("a.tsx", '<path fill="red" stroke=\'blue\' />')).toEqual(["1:named-color"]);
    expect(rules("a.tsx", '<stop stopColor="gold" floodColor="navy" />')).toEqual(["1:named-color"]);
    expect(rules("a.tsx", '<Icon color="white" />')).toEqual(["1:named-color"]);
    expect(rules("a.tsx", '<path fill="currentColor" stroke="none" color="var(--sk-color-ink)" />')).toEqual([]);
  });

  it("checks vendor-prefixed style keys", () => {
    expect(rules("a.tsx", 'const s = { WebkitBackdropFilter: "blur(8px)" };')).toEqual(["1:filter-literal"]);
    expect(rules("a.tsx", 'const s = { WebkitBackdropFilter: "blur(var(--sk-blur-bar))" };')).toEqual([]);
    expect(rules("a.tsx", "const s = { MozBorderRadius: 4 };")).toEqual(["1:radius-literal"]);
    expect(rules("a.tsx", 'const s = { msFilter: "blur(2px)" };')).toEqual(["1:filter-literal"]);
    expect(rules("a.css", ".a { -webkit-border-radius: 4px; -moz-border-radius: 0; }")).toEqual(["1:radius-literal"]);
  });

  it("flags CSS system colours", () => {
    for (const word of ["Canvas", "CanvasText", "ButtonFace", "ButtonText", "Field", "FieldText", "Highlight", "HighlightText", "LinkText", "GrayText"]) {
      expect(rules("a.css", `.a { color: ${word}; }`)).toEqual(["1:named-color"]);
    }
    expect(rules("a.css", ".a { border: 1px solid canvastext; }")).toEqual(["1:named-color"]);
  });

  it("flags the full CSS named-colour list, but not transparent or currentcolor", () => {
    expect(rules("a.css", ".a { color: rebeccapurple; }")).toEqual(["1:named-color"]);
    expect(rules("a.css", ".a { background: lightgoldenrodyellow; }")).toEqual(["1:named-color"]);
    expect(rules("a.css", ".a { color: transparent; fill: currentcolor; }")).toEqual([]);
  });

  it("flags quoted style-object keys with matching quotes", () => {
    expect(rules("a.tsx", 'const s = { "borderColor": "red" };')).toEqual(["1:named-color"]);
    expect(rules("a.tsx", "const s = { 'color': 'red' };")).toEqual(["1:named-color"]);
    expect(rules("a.tsx", 'const s = { "color": "var(--sk-color-ink)" };')).toEqual([]);
    expect(rules("a.tsx", 'const s = { "border-color": "red" };')).toEqual(["1:named-color"]);
    expect(rules("a.tsx", "const s = { 'background-color': 'red' };")).toEqual(["1:named-color"]);
  });

  it("flags colour attributes given as a braced string", () => {
    expect(rules("a.tsx", '<path fill={"red"} />')).toEqual(["1:named-color"]);
    expect(rules("a.tsx", "<path stroke={'blue'} />")).toEqual(["1:named-color"]);
    expect(rules("a.tsx", '<path fill={"none"} />')).toEqual([]);
  });

  it("checks logical border colours", () => {
    for (const prop of ["border-inline", "border-block-start", "border-inline-end-color", "border-block-color"]) {
      expect(rules("a.css", `.a { ${prop}: 1px solid red; }`)).toEqual(["1:named-color"]);
    }
    expect(rules("a.tsx", 'const s = { borderInlineStartColor: "red" };')).toEqual(["1:named-color"]);
    expect(rules("a.css", ".a { border-inline-start: 1px solid var(--sk-color-ink); }")).toEqual([]);
  });

  it("ignores hex-looking fragments that are not colours", () => {
    expect(rules("a.css", "#fade {\n  margin: 0;\n}")).toEqual([]);
    expect(rules("a.css", "#abc, #def { margin: 0; }")).toEqual([]);
    expect(rules("a.css", ".a { mask: url(#abc); fill: url(#fff); }")).toEqual([]);
    expect(rules("a.tsx", "const css = `#fade { margin: 0 }`;")).toEqual([]);
    expect(rules("a.tsx", 'const u = "url(#abc)";')).toEqual([]);
  });

  it("still flags real hex colours in a list and reports the right line in a wrapped value", () => {
    expect(rules("a.css", ".a { box-shadow: 0 0 #fff, 0 0 #000; }")).toEqual(["1:hex-color"]);
    expect(rules("a.css", ".a {\n  box-shadow:\n    0 0 4px var(--sk-color-ink),\n    0 0 #000;\n}")).toEqual(["4:hex-color"]);
  });

  it("checks the font shorthand", () => {
    expect(rules("a.css", ".a { font: 12px Georgia, serif; }")).toEqual(["1:font-literal"]);
    expect(rules("a.css", ".a { font: 12px/1.4 Georgia, var(--sk-font-serif); }")).toEqual(["1:font-literal"]);
    expect(rules("a.css", '.a { font: italic bold 12px/1.4 "Foo"; }')).toEqual(["1:font-literal"]);
    expect(rules("a.css", ".a { font: italic bold 0.9rem/1.4 var(--sk-font-sans); }")).toEqual([]);
    expect(rules("a.css", ".a { font: inherit; }")).toEqual([]);
    expect(rules("a.tsx", 'const s = { font: "12px Georgia" };')).toEqual(["1:font-literal"]);
  });

  it("treats custom properties as carrying a colour", () => {
    expect(rules("a.css", ".a { --accent: red; }")).toEqual(["1:named-color"]);
    expect(rules("a.css", ".a { --c2: #fff; }")).toEqual(["1:hex-color"]);
    expect(rules("a.css", ".a { --thumb: 12px; --c: var(--sk-color-ink); }")).toEqual([]);
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

describe("color( in script and CSS", () => {
  it("passes a plain color(page) call in a .tsx", () => {
    expect(rules("a.tsx", "const c = color(page);")).toEqual([]);
  });

  it("flags a CSS colour space call in a .tsx and in CSS", () => {
    expect(rules("a.tsx", "const c = 'color(display-p3 1 0 0)';")).toEqual(["1:color-function"]);
    expect(rules("a.css", ".a { color: color(display-p3 1 0 0); }")).toEqual(["1:color-function"]);
  });
});
