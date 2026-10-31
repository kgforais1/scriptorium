// Pure token-to-CSS conversion for skin files (DTCG). The build script scripts/build-skin-css.mjs
// does the file I/O and calls into this module, which Node loads directly (type stripping), so it
// must stay self-contained: no imports, no enums.

/** One resolved token: its dotted path, its effective `$type`, and the raw `$value`. */
export type SkinToken = { path: string; type: string; value: unknown };

const GENERIC_FAMILIES = new Set([
  "serif",
  "sans-serif",
  "monospace",
  "cursive",
  "fantasy",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
  "math",
  "emoji",
  "fangsong",
  // System-font keywords. Quoting them turns them into plain family names that no longer match.
  "-apple-system",
  "BlinkMacSystemFont",
]);

const DIMENSION_UNITS = new Set(["px", "rem"]);
const DURATION_UNITS = new Set(["ms", "s"]);

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function colorToCss(path: string, value: unknown): string {
  if (typeof value !== "string" || value === "") throw new Error(`${path}: a color must be a non-empty string`);
  return value;
}

// DTCG writes {value, unit}. A plain string such as "10px" is accepted as written.
function quantityToCss(path: string, value: unknown, units: Set<string>): string {
  if (typeof value === "string" && value !== "") return value;
  if (isRecord(value) && typeof value.value === "number" && Number.isFinite(value.value) && typeof value.unit === "string") {
    if (!units.has(value.unit)) throw new Error(`${path}: unit "${value.unit}" is not one of ${[...units].join(", ")}`);
    return `${value.value}${value.unit}`;
  }
  throw new Error(`${path}: expected {value, unit} or a string`);
}

function numberToCss(path: string, value: unknown): string {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`${path}: a number must be a finite number`);
  return String(value);
}

function cubicBezierToCss(path: string, value: unknown): string {
  if (!Array.isArray(value) || value.length !== 4 || !value.every((n) => typeof n === "number" && Number.isFinite(n))) {
    throw new Error(`${path}: a cubicBezier must be an array of four numbers`);
  }
  return `cubic-bezier(${value.join(", ")})`;
}

function fontFamilyToCss(path: string, value: unknown): string {
  const names = typeof value === "string" ? [value] : value;
  if (!Array.isArray(names) || names.length === 0 || !names.every((n) => typeof n === "string" && n !== "")) {
    throw new Error(`${path}: a fontFamily must be a non-empty list of names`);
  }
  return names.map((n: string) => (GENERIC_FAMILIES.has(n) ? n : quoteFamily(n))).join(", ");
}

const BACKSLASH = String.fromCodePoint(92);

/** Quotes a font family name for CSS, escaping backslashes and double quotes. */
function quoteFamily(name: string): string {
  const escaped = name.split(BACKSLASH).join(BACKSLASH + BACKSLASH).split('"').join(BACKSLASH + '"');
  return '"' + escaped + '"';
}

/** Converts one token value to its CSS text. An unknown `$type` throws. */
export function tokenToCss(path: string, type: string, value: unknown): string {
  switch (type) {
    case "color":
      return colorToCss(path, value);
    case "dimension":
      return quantityToCss(path, value, DIMENSION_UNITS);
    case "duration":
      return quantityToCss(path, value, DURATION_UNITS);
    case "number":
      return numberToCss(path, value);
    case "cubicBezier":
      return cubicBezierToCss(path, value);
    case "fontFamily":
      return fontFamilyToCss(path, value);
    default:
      throw new Error(`${path}: unknown $type "${type}"`);
  }
}

function typeOf(node: Record<string, unknown>, path: string, inherited: string | undefined): string | undefined {
  if (!("$type" in node)) return inherited;
  if (typeof node.$type !== "string") throw new Error(`${path || "tokens file"}: $type must be a string`);
  return node.$type;
}

function walk(node: Record<string, unknown>, trail: string[], inherited: string | undefined, out: SkinToken[]): void {
  const path = trail.join(".");
  const type = typeOf(node, path, inherited);
  if ("$value" in node) {
    if (!type) throw new Error(`${path}: token has no $type, own or inherited`);
    out.push({ path, type, value: node.$value });
    return;
  }
  for (const [key, child] of Object.entries(node)) {
    if (key.startsWith("$")) continue;
    if (!isRecord(child)) throw new Error(`${[...trail, key].join(".")}: expected a token or a group`);
    walk(child, [...trail, key], type, out);
  }
}

/** Flattens a DTCG tree into tokens in file order. `$type` is inherited and a leaf overrides it. */
export function flattenTokens(tree: unknown): SkinToken[] {
  if (!isRecord(tree)) throw new Error("tokens file: expected a JSON object");
  const out: SkinToken[] = [];
  const rootType = typeOf(tree, "", undefined);
  for (const [key, child] of Object.entries(tree)) {
    if (key.startsWith("$")) continue;
    if (!isRecord(child)) throw new Error(`${key}: expected a token or a group`);
    walk(child, [key], rootType, out);
  }
  return out;
}

/** The CSS custom property name for a token path: `color.ink-muted` becomes `--sk-color-ink-muted`. */
export function propertyName(path: string): string {
  return `--sk-${path.replace(/\./g, "-")}`;
}

/** A required token: its path and the `$type` it must have. */
export type RequiredToken = { path: string; type: string };

/**
 * Errors for a skin's tokens against `required`: a token that is missing, unexpected, defined
 * twice (a dotted key such as "color.ink" next to a nested `color { ink }` flattens to the same
 * path), or of the wrong `$type`.
 */
export function checkRequired(tokens: readonly RequiredToken[], required: readonly RequiredToken[]): string[] {
  const want = new Map<string, string>();
  const errors: string[] = [];
  for (const r of required) {
    if (want.has(r.path)) errors.push(`${r.path}: listed more than once in required-tokens.json`);
    want.set(r.path, r.type);
  }
  const seen = new Set<string>();
  for (const t of tokens) {
    if (seen.has(t.path)) {
      errors.push(`${t.path}: defined more than once`);
      continue;
    }
    seen.add(t.path);
    const type = want.get(t.path);
    if (type === undefined) errors.push(`${t.path}: unexpected token (not in required-tokens.json)`);
    else if (type !== t.type) errors.push(`${t.path}: expected $type ${type}, got ${t.type}`);
  }
  for (const r of required) if (!seen.has(r.path)) errors.push(`${r.path}: required token is missing`);
  return errors;
}

/** Renders the generated stylesheet for one skin from its parsed tokens.json. */
export function buildSkinCss(id: string, tree: unknown): string {
  const tokens = flattenTokens(tree);
  const lines = tokens.map((t) => `  ${propertyName(t.path)}: ${tokenToCss(t.path, t.type, t.value)};`);
  return [
    `/* Generated from design/skins/${id}/tokens.json by scripts/build-skin-css.mjs. Do not edit. */`,
    ":root {",
    ...lines,
    "}",
    "",
  ].join("\n");
}
