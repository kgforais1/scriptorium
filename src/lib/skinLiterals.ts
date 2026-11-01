// Finds skin-owned literals (colours, font families, radii, filters) in reader source. Skins
// supply those through --sk-* custom properties, so a literal in a component is a bug.
// scripts/check-skin-literals.mjs does the file walking and calls into this module, which Node
// loads directly (type stripping): keep it self-contained, with no imports and no enums.

/** One literal found in a source file. */
export type SkinViolation = { line: number; rule: string; text: string };

const NAMED_COLORS = [
  // The 148 CSS named colours. `transparent` and `currentcolor` are left out: they are not skin colours.
  "aliceblue", "antiquewhite", "aqua", "aquamarine", "azure", "beige", "bisque", "black",
  "blanchedalmond", "blue", "blueviolet", "brown", "burlywood", "cadetblue", "chartreuse",
  "chocolate", "coral", "cornflowerblue", "cornsilk", "crimson", "cyan", "darkblue", "darkcyan",
  "darkgoldenrod", "darkgray", "darkgreen", "darkgrey", "darkkhaki", "darkmagenta",
  "darkolivegreen", "darkorange", "darkorchid", "darkred", "darksalmon", "darkseagreen",
  "darkslateblue", "darkslategray", "darkslategrey", "darkturquoise", "darkviolet", "deeppink",
  "deepskyblue", "dimgray", "dimgrey", "dodgerblue", "firebrick", "floralwhite", "forestgreen",
  "fuchsia", "gainsboro", "ghostwhite", "gold", "goldenrod", "gray", "green", "greenyellow",
  "grey", "honeydew", "hotpink", "indianred", "indigo", "ivory", "khaki", "lavender",
  "lavenderblush", "lawngreen", "lemonchiffon", "lightblue", "lightcoral", "lightcyan",
  "lightgoldenrodyellow", "lightgray", "lightgreen", "lightgrey", "lightpink", "lightsalmon",
  "lightseagreen", "lightskyblue", "lightslategray", "lightslategrey", "lightsteelblue",
  "lightyellow", "lime", "limegreen", "linen", "magenta", "maroon", "mediumaquamarine",
  "mediumblue", "mediumorchid", "mediumpurple", "mediumseagreen", "mediumslateblue",
  "mediumspringgreen", "mediumturquoise", "mediumvioletred", "midnightblue", "mintcream",
  "mistyrose", "moccasin", "navajowhite", "navy", "oldlace", "olive", "olivedrab", "orange",
  "orangered", "orchid", "palegoldenrod", "palegreen", "paleturquoise", "palevioletred",
  "papayawhip", "peachpuff", "peru", "pink", "plum", "powderblue", "purple", "rebeccapurple",
  "red", "rosybrown", "royalblue", "saddlebrown", "salmon", "sandybrown", "seagreen", "seashell",
  "sienna", "silver", "skyblue", "slateblue", "slategray", "slategrey", "snow", "springgreen",
  "steelblue", "tan", "teal", "thistle", "tomato", "turquoise", "violet", "wheat", "white",
  "whitesmoke", "yellow", "yellowgreen",
  // CSS system colours. They follow the OS, not the skin, so a skin must not lean on them.
  "Canvas", "CanvasText", "ButtonFace", "ButtonText", "ButtonBorder", "Field", "FieldText",
  "Highlight", "HighlightText", "LinkText", "VisitedText", "ActiveText", "GrayText", "Mark",
  "MarkText", "AccentColor", "AccentColorText", "SelectedItem", "SelectedItemText",
].join("|");

const COLOR_PROP_NAMES = new Set([
  "color", "background", "background-color", "background-image", "outline", "outline-color", "fill",
  "stroke", "stop-color", "flood-color", "lighting-color", "box-shadow", "text-shadow", "caret-color",
  "accent-color", "text-decoration-color",
]);
// Physical sides, and the logical inline and block sides with their -start and -end forms.
const BORDER_SIDES = new Set([
  "border", "border-top", "border-right", "border-bottom", "border-left", "border-inline",
  "border-inline-start", "border-inline-end", "border-block", "border-block-start", "border-block-end",
]);
function isBorderColorProp(prop: string): boolean {
  return BORDER_SIDES.has(prop.replace(/-color$/, ""));
}

/** Matches #rgb, #rgba, #rrggbb, #rrggbbaa. Exported for tests. */
export const HEX_COLOR = /#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})(?![0-9a-z_-])/i;
const NAMED_COLOR_FUNCTION = /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(/;
// `color(` is also a plain function name in TS, so it counts only when a colour space or `from`
// follows. A hyphenated space such as display-p3 or xyz-d50 matches on its first word.
const COLOR_SPACE_FUNCTION = /\bcolor\(\s*(?:srgb|display-p3|a98-rgb|prophoto-rgb|rec2020|xyz|from)\b/;
/** Matches a CSS colour function call. Exported for tests. */
export const COLOR_FUNCTION = new RegExp(`${NAMED_COLOR_FUNCTION.source}|${COLOR_SPACE_FUNCTION.source}`, "i");
const NAMED_COLOR_WORD = new RegExp(String.raw`(?<![\w-])(?:${NAMED_COLORS})(?![\w-])`, "i");
const CSS_DECL = /(?<![\w-])([\w-]+)\s*:\s*([^;{}]+)/g;
// A JS style-object entry: a camelCase or PascalCase (vendor-prefixed) key, bare or in matching
// quotes, and a colon. Group 2 is the key. The literal after it is read by hand.
// A quoted key may be kebab-case ("border-color"); a bare key cannot hold a hyphen.
const JS_KEY = /(?<![\w-])(?:(["'])([A-Za-z][\w-]*)\1|([A-Za-z]\w*))\s*:\s*/g;
// A JSX or SVG presentation attribute that carries a colour.
// The string may sit in braces, as in fill={"red"}.
const COLOR_ATTR = /(?<![\w-])(fill|stroke|color|(?:stop|flood)-?[cC]olor)\s*=\s*(?:\{\s*)?["']([^"'\n]*)["']/g;
const HEX_GLOBAL = new RegExp(HEX_COLOR.source, "gi");
const COLOR_FUNCTION_GLOBAL = new RegExp(COLOR_FUNCTION.source, "gi");
const JS_NUMBER = /-?[\d.]+/y;

// Reads a string or number literal that starts at `pos`, or returns null for anything else.
function literalAt(text: string, pos: number): string | null {
  const quote = text[pos];
  if (quote === '"' || quote === "'" || quote === "`") {
    const end = text.indexOf(quote, pos + 1);
    const nl = text.indexOf("\n", pos + 1);
    if (end === -1 || (nl !== -1 && nl < end)) return null;
    return text.slice(pos + 1, end);
  }
  JS_NUMBER.lastIndex = pos;
  return JS_NUMBER.exec(text)?.[0] ?? null;
}

function stripVars(value: string): string {
  return value.replace(/var\([^)]*\)/g, "");
}

// Custom properties may hold any colour, so they are checked like a colour property.
function carriesColor(prop: string): boolean {
  return prop.startsWith("--") || COLOR_PROP_NAMES.has(prop) || isBorderColorProp(prop);
}

function usesNamedColor(prop: string, value: string): boolean {
  return carriesColor(prop) && NAMED_COLOR_WORD.test(stripVars(value).replace(/url\([^)]*\)/g, ""));
}

function badFontFamily(value: string): boolean {
  const v = value.trim();
  return !/^(?:var\(--sk-[\w-]+\)|inherit|initial|unset)$/.test(v);
}

const FONT_SHORTHAND_WORDS = new Set([
  "normal", "italic", "oblique", "bold", "bolder", "lighter", "small-caps", "condensed", "expanded",
  "xx-small", "x-small", "small", "medium", "large", "x-large", "xx-large", "larger", "smaller",
]);

// The font shorthand names a family, so its family part must come from a --sk-* variable. What is
// left after removing var() calls may only be a style, weight, size, or line height.
function badFontShorthand(value: string): boolean {
  const v = value.trim();
  if (/^(?:inherit|initial|unset)$/.test(v)) return false;
  if (!v.includes("var(--sk-")) return true;
  const rest = stripVars(v).split(/[\s/,]+/).filter((t) => t !== "");
  return rest.some((t) => !FONT_SHORTHAND_WORDS.has(t.toLowerCase()) && !/^-?[\d.]+[a-z%]*$/i.test(t));
}

function badRadius(value: string): boolean {
  const rest = stripVars(value).replace(/!important/i, "");
  return (rest.match(/-?\d*\.?\d+[a-z%]*/gi) ?? []).some((t) => Number.parseFloat(t) !== 0);
}

// Splits a filter value into its top-level calls, such as `blur(var(--x))`, by tracking depth.
function topLevelCalls(value: string): string[] {
  const calls: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < value.length; i++) {
    if (value[i] === "(") depth++;
    else if (value[i] === ")" && depth > 0 && --depth === 0) {
      calls.push(value.slice(start, i + 1).trim());
      start = i + 1;
    }
  }
  return calls.length > 0 ? calls : [value];
}

function badFilter(value: string): boolean {
  const v = value.trim();
  if (/^(?:none|inherit|initial|unset)$/.test(v)) return false;
  // Each function call must take its argument from a variable.
  return topLevelCalls(v).some((call) => !call.includes("var(--sk-"));
}

// A declaration is checked by its normalised (kebab-case) property name, without a vendor prefix.
function checkDeclaration(name: string, value: string): string | null {
  const prop = name.replace(/^-(?:webkit|moz|ms)-/, "");
  if (/^(?:backdrop-)?filter$/.test(prop) && badFilter(value)) return "filter-literal";
  if (prop === "font-family" && badFontFamily(value)) return "font-family-literal";
  if (prop === "font" && badFontShorthand(value)) return "font-literal";
  if (/^border(?:-[a-z]+){0,2}-radius$/.test(prop) && badRadius(value)) return "radius-literal";
  if (usesNamedColor(prop, value)) return "named-color";
  return null;
}

// `WebkitBackdropFilter` becomes `-webkit-backdrop-filter`, `borderRadius` becomes `border-radius`.
function kebab(name: string): string {
  const vendor = /^(Webkit|Moz|ms)(?=[A-Z])/.exec(name);
  const rest = vendor ? name.slice(vendor[0].length) : name;
  const dashed = rest.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`).replace(/^-/, "");
  return vendor ? `-${vendor[0].toLowerCase()}-${dashed}` : dashed;
}

function lineOf(text: string, index: number): number {
  let n = 1;
  for (let i = text.indexOf("\n"); i !== -1 && i < index; i = text.indexOf("\n", i + 1)) n++;
  return n;
}

type Hit = { index: number; rule: string };

// Colour literals in a stretch of text. `url(#abc)` and a selector such as `#fade {` are not colours.
function colorLiteralHits(segment: string, base: number): Hit[] {
  const text = segment.replace(/url\([^)]*\)/g, (u) => " ".repeat(u.length));
  const hits: Hit[] = [];
  for (const m of text.matchAll(HEX_GLOBAL)) {
    if (!/^\s*\{/.test(text.slice(m.index + m[0].length))) hits.push({ index: base + m.index, rule: "hex-color" });
  }
  for (const m of text.matchAll(COLOR_FUNCTION_GLOBAL)) hits.push({ index: base + m.index, rule: "color-function" });
  return hits;
}

function cssHits(text: string): Hit[] {
  const hits: Hit[] = [];
  for (const m of text.matchAll(CSS_DECL)) {
    const rule = checkDeclaration(m[1].toLowerCase(), m[2]);
    if (rule) hits.push({ index: m.index, rule });
    // Colour literals count only in a declaration value, never in a selector.
    hits.push(...colorLiteralHits(m[2], m.index + m[0].length - m[2].length));
  }
  return hits;
}

function scriptHits(text: string): Hit[] {
  const hits = colorLiteralHits(text, 0);
  for (const m of text.matchAll(JS_KEY)) {
    const value = literalAt(text, m.index + m[0].length);
    const rule = value === null ? null : checkDeclaration(kebab(m[2] ?? m[3]), value);
    if (rule) hits.push({ index: m.index, rule });
  }
  for (const m of text.matchAll(COLOR_ATTR)) {
    if (usesNamedColor(kebab(m[1]), m[2])) hits.push({ index: m.index, rule: "named-color" });
  }
  return hits;
}

// Blank out comments but keep newlines, so reported line numbers stay right.
function stripBlockComments(text: string): string {
  return text.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " "));
}

function stripLineComments(text: string): string {
  return text.replace(/(^|[\s;,{(])\/\/.*$/gm, (_m, lead: string) => lead);
}

/** Whether a path is subject to the skin-literal check. Engine code and tests are exempt. */
export function isCheckedFile(path: string): boolean {
  const p = path.replace(/\\/g, "/");
  if (!/\.(?:css|tsx?)$/.test(p)) return false;
  if (/\.test\.tsx?$/.test(p)) return false;
  return !p.includes("/engine/");
}

/** Returns every skin literal in one file's text, with 1-based line numbers. */
export function findSkinViolations(path: string, text: string): SkinViolation[] {
  const isCss = path.endsWith(".css");
  const noBlocks = stripBlockComments(text);
  const clean = isCss ? noBlocks : stripLineComments(noBlocks);
  const lines = clean.split("\n");
  const found = new Map<string, SkinViolation>();
  const add = (line: number, rule: string) => {
    found.set(`${line}:${rule}`, { line, rule, text: lines[line - 1].trim() });
  };
  for (const hit of (isCss ? cssHits(clean) : scriptHits(clean))) add(lineOf(clean, hit.index), hit.rule);
  return [...found.values()].sort((a, b) => a.line - b.line);
}
