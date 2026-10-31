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
    // A Windows checkout may have turned LF into CRLF. Line endings are not a difference.
    if (current?.replaceAll("\r\n", "\n") === css) return null;
    return `src/styles/skin-${id}.css is stale. Run \`pnpm skin:css\` and commit the result.`;
  }
  mkdirSync(outDir, { recursive: true });
  writeFileSync(target, css);
  console.log(`wrote src/styles/skin-${id}.css`);
  return null;
}

// Generated files whose skin no longer has a tokens.json.
function orphans(ids) {
  if (!existsSync(outDir)) return [];
  return readdirSync(outDir)
    .map((name) => /^skin-(.+)\.css$/.exec(name)?.[1])
    .filter((id) => id !== undefined && !ids.includes(id))
    .map((id) => `src/styles/skin-${id}.css has no design/skins/${id}/tokens.json. Delete it.`);
}

function main() {
  const required = readJson(join(skinsDir, "required-tokens.json")).required;
  const failures = [];
  const ids = skinIds();
  if (checkOnly) failures.push(...orphans(ids));
  for (const id of ids) {
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
