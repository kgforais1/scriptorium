// Finds skin-owned literals (colours, font families, radii, filters) in reader source. Skins
// supply those through --sk-* custom properties, so a literal in a component is a bug.
// scripts/check-skin-literals.mjs does the file walking and calls into this module, which Node
// loads directly (type stripping): keep it self-contained, with no imports and no enums.

/** One literal found in a source file. */
export type SkinViolation = { line: number; rule: string; text: string };

const NAMED_COLORS = [
  "aqua", "black", "blue", "brown", "cyan", "fuchsia", "gold", "gray", "green", "grey", "lime",
  "magenta", "maroon", "navy", "olive", "orange", "pink", "purple", "red", "silver", "teal",
  "white", "yellow",
].join("|");

const COLOR_PROPS =
  "color|background|background-color|border|border-(?:top|right|bottom|left)(?:-color)?|border-color|outline|outline-color|fill|stroke|box-shadow|text-shadow|caret-color|accent-color|text-decoration-color";

/** Matches #rgb, #rgba, #rrggbb, #rrggbbaa. Exported for tests. */
export const HEX_COLOR = /#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})(?![0-9a-z_-])/i;
/** Matches a CSS colour function call. Exported for tests. */
export const COLOR_FUNCTION = /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(/i;
const NAMED_COLOR_WORD = new RegExp(String.raw`(?<![\w-])(?:${NAMED_COLORS})(?![\w-])`, "i");
const CSS_DECL = /(?<![\w-])([a-z-]+)\s*:\s*([^;{}]+)/gi;
// A JS style-object entry: a camelCase key and a colon. The literal after it is read by hand.
const JS_KEY = /\b([a-z][A-Za-z]*)\s*:\s*/g;
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

function usesNamedColor(prop: string, value: string): boolean {
  return new RegExp(`^(?:${COLOR_PROPS})$`).test(prop) && NAMED_COLOR_WORD.test(stripVars(value));
}

function badFontFamily(value: string): boolean {
  const v = value.trim();
  return !/^(?:var\(--sk-[\w-]+\)|inherit|initial|unset)$/.test(v);
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

// A declaration is checked by its normalised (kebab-case) property name.
function checkDeclaration(prop: string, value: string): string | null {
  if (/^(?:-webkit-)?(?:backdrop-)?filter$/.test(prop) && badFilter(value)) return "filter-literal";
  if (prop === "font-family" && badFontFamily(value)) return "font-family-literal";
  if (/^border(?:-[a-z]+){0,2}-radius$/.test(prop) && badRadius(value)) return "radius-literal";
  if (usesNamedColor(prop, value)) return "named-color";
  return null;
}

function kebab(name: string): string {
  return name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
}

function lineOf(text: string, index: number): number {
  let n = 1;
  for (let i = text.indexOf("\n"); i !== -1 && i < index; i = text.indexOf("\n", i + 1)) n++;
  return n;
}

// Declarations are matched over the whole text, so a value that wraps onto the next line is seen.
function declarationHits(text: string, isCss: boolean): { index: number; rule: string }[] {
  const hits: { index: number; rule: string }[] = [];
  if (isCss) {
    for (const m of text.matchAll(CSS_DECL)) {
      const rule = checkDeclaration(m[1].toLowerCase(), m[2]);
      if (rule) hits.push({ index: m.index, rule });
    }
    return hits;
  }
  for (const m of text.matchAll(JS_KEY)) {
    const value = literalAt(text, m.index + m[0].length);
    const rule = value === null ? null : checkDeclaration(kebab(m[1]), value);
    if (rule) hits.push({ index: m.index, rule });
  }
  return hits;
}

// Blank out comments but keep newlines, so reported line numbers stay right.
function stripComments(text: string, isCss: boolean): string {
  const blank = (s: string) => s.replace(/[^\n]/g, " ");
  let out = text.replace(/\/\*[\s\S]*?\*\//g, blank);
  if (!isCss) out = out.replace(/(^|[\s;,{(])\/\/.*$/gm, (_m, lead: string) => lead);
  return out;
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
  const clean = stripComments(text, isCss);
  const lines = clean.split("\n");
  const found = new Map<string, SkinViolation>();
  const add = (line: number, rule: string) => {
    found.set(`${line}:${rule}`, { line, rule, text: lines[line - 1].trim() });
  };
  lines.forEach((line, i) => {
    if (HEX_COLOR.test(line)) add(i + 1, "hex-color");
    if (COLOR_FUNCTION.test(line)) add(i + 1, "color-function");
  });
  for (const hit of declarationHits(clean, isCss)) add(lineOf(clean, hit.index), hit.rule);
  return [...found.values()].sort((a, b) => a.line - b.line);
}
