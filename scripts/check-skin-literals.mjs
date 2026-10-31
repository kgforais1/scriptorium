// Fails when reader source hard-codes something a skin owns: colours, font families, border
// radii, or filter values. Those must come from --sk-* custom properties. Pixel sizes for layout
// are fine. Engine code (WebGL, reads scene.json) and tests are exempt. Rules live in
// src/lib/skinLiterals.ts so they are unit-tested; Node loads it directly.
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { findSkinViolations, isCheckedFile } from "../src/lib/skinLiterals.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const readerDir = join(root, "src", "features", "reader");

function* walk(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(path);
    else yield path;
  }
}

let count = 0;
for (const path of walk(readerDir)) {
  const rel = relative(root, path).replaceAll("\\", "/");
  if (!isCheckedFile(rel)) continue;
  for (const v of findSkinViolations(rel, readFileSync(path, "utf8"))) {
    console.error(`${rel}:${v.line}: ${v.rule}: ${v.text}`);
    count++;
  }
}
if (count > 0) {
  console.error(`\n${count} skin literal(s) found. Use a --sk-* custom property instead.`);
  process.exit(1);
}
console.log("no skin literals in src/features/reader");
