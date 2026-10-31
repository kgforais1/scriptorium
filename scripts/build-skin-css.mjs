// Builds src/styles/skin-<id>.css from design/skins/<id>/tokens.json (DTCG), one :root block of
// --sk-* custom properties per skin. Skins without a tokens.json are skipped.
//   node scripts/build-skin-css.mjs          write the files
//   node scripts/build-skin-css.mjs --check  exit 1 if a committed file is stale
// The conversion lives in src/lib/skinTokens.ts so it is unit-tested; Node loads it directly.
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildSkinCss, checkRequired, flattenTokens } from "../src/lib/skinTokens.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const skinsDir = join(root, "design", "skins");
const outDir = join(root, "src", "styles");
const checkOnly = process.argv.includes("--check");

const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));

function skinIds() {
  return readdirSync(skinsDir, { withFileTypes: true })
    .filter((e) => e.isDirectory() && existsSync(join(skinsDir, e.name, "tokens.json")))
    .map((e) => e.name)
    .sort();
}

// Returns the CSS for one skin, or throws with every problem found.
function render(id, required) {
  const tree = readJson(join(skinsDir, id, "tokens.json"));
  const problems = checkRequired(flattenTokens(tree).map((t) => t.path), required);
  if (problems.length > 0) throw new Error(problems.join("\n"));
  return buildSkinCss(id, tree);
}

function buildOne(id, required) {
  const target = join(outDir, `skin-${id}.css`);
  const css = render(id, required);
  if (checkOnly) {
    const current = existsSync(target) ? readFileSync(target, "utf8") : null;
    if (current === css) return null;
    return `src/styles/skin-${id}.css is stale. Run \`pnpm skin:css\` and commit the result.`;
  }
  mkdirSync(outDir, { recursive: true });
  writeFileSync(target, css);
  console.log(`wrote src/styles/skin-${id}.css`);
  return null;
}

function main() {
  const required = readJson(join(skinsDir, "required-tokens.json")).required;
  const failures = [];
  for (const id of skinIds()) {
    try {
      const stale = buildOne(id, required);
      if (stale) failures.push(stale);
    } catch (err) {
      failures.push(`design/skins/${id}/tokens.json:\n${err.message}`);
    }
  }
  if (failures.length > 0) {
    console.error(failures.join("\n"));
    process.exit(1);
  }
  if (checkOnly) console.log("skin CSS is current");
}

main();
