#!/usr/bin/env node
// Writes THIRD_PARTY_LICENSES.generated.md: the licence texts of the Rust crates
// (via cargo-about) and the production npm packages (via pnpm), for the app bundle.
//
//   node scripts/gen-licenses.mjs           write the file
//   node scripts/gen-licenses.mjs --check   fail if the file on disk is stale
//
// Needs the cargo-about version in package.json `config.cargoAboutVersion`; CI reads the same field.
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outFile = join(root, "THIRD_PARTY_LICENSES.generated.md");
const check = process.argv.includes("--check");

function fail(message) {
  console.error(`gen-licenses: ${message}`);
  process.exit(1);
}

// Byte order, not locale order, so every machine writes the same file.
function byCodePoint(a, b) {
  if (a === b) return 0;
  return a < b ? -1 : 1;
}

function run(cmd, args, cwd) {
  // pnpm is a .cmd shim on Windows, which spawnSync only finds through a shell.
  const shell = process.platform === "win32";
  const r = spawnSync(cmd, args, { cwd, shell, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  if (r.error?.code === "ENOENT") return { missing: true };
  return { status: r.status, out: r.stdout ?? "", err: r.stderr || r.error?.message || "" };
}

function mustRun(cmd, args, cwd) {
  const r = run(cmd, args, cwd);
  if (!r.missing && r.status !== 0) fail(`\`${cmd} ${args.join(" ")}\` failed:\n${r.err}`);
  return r;
}

// One pin for local runs and CI; another cargo-about version may format the file differently.
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const CARGO_ABOUT_VERSION = pkg.config?.cargoAboutVersion;
if (!CARGO_ABOUT_VERSION) fail("package.json is missing config.cargoAboutVersion, the cargo-about version to use");
const INSTALL = `cargo install cargo-about --locked --version ${CARGO_ABOUT_VERSION} --features cli`;

/** Rust crates: cargo-about renders src-tauri/about.hbs under src-tauri/about.toml. */
function rustSection() {
  const v = run("cargo", ["about", "--version"], root);
  if (v.missing || v.status !== 0) fail(`cargo-about is not installed. Run: ${INSTALL}`);
  const found = /^cargo-about\s+(\S+)/m.exec(v.out)?.[1] ?? v.out.trim();
  if (found !== CARGO_ABOUT_VERSION) {
    fail(`cargo-about ${found} is installed, but the notices need ${CARGO_ABOUT_VERSION}. Run: ${INSTALL}`);
  }
  const r = run("cargo", ["about", "generate", "-c", "about.toml", "about.hbs"], join(root, "src-tauri"));
  if (r.status !== 0) fail(`\`cargo about generate\` failed:\n${r.err}`);
  // Licence texts carry CRLF and trailing blanks; normalise so the file is stable.
  return r.out.split(/\r?\n/).map((l) => l.trimEnd()).join("\n").trim();
}

const normalise = (text) => text.split(/\r?\n/).map((l) => l.trimEnd()).join("\n").trim();
const LICENSE_FILE = /^(licen[cs]e|copying)([._-]|$)/i;

/** Standard texts in scripts/license-texts/ for a package that ships only an SPDX stub. */
function spdxFallback(name, dir, stub) {
  const spdx = readFileSync(join(dir, stub), "utf8");
  // A declared licence may be an expression such as "MIT OR Apache-2.0"; load every id in it.
  const declared = [...spdx.matchAll(/^PackageLicenseDeclared:(.*)$/gm)].map((m) => m[1]);
  // A WITH exception id stays in, so it needs a text file too and cannot vanish from the notice.
  const terms = declared.flatMap((d) => d.replaceAll(/[()]/g, " ").split(/\s+/));
  const ids = [...new Set(terms)].filter((id) => id && !["AND", "OR", "WITH"].includes(id));
  const copyright = spdx.match(/^PackageCopyrightText:(.*)$/m)?.[1]?.trim();
  if (!ids.length || !copyright) fail(`npm package ${name}: ${stub} lacks a licence or copyright line`);
  const texts = ids.toSorted(byCodePoint).map((id) => {
    const file = join(root, "scripts", "license-texts", `${id}.txt`);
    if (!existsSync(file)) fail(`npm package ${name}: add the ${id} text as ${file}`);
    return normalise(readFileSync(file, "utf8").replace("{{copyright}}", () => copyright));
  });
  return { note: `The package ships only ${stub}. These are the standard texts of its licences.`, texts };
}

/** Every licence text a package ships (LICENSE, LICENSE-MIT, COPYING, ...); SPDX stubs fall back. */
function licenseTexts(name, dir) {
  const files = readdirSync(dir).sort(byCodePoint).filter((f) => LICENSE_FILE.test(f));
  const texts = files.filter((f) => !/\.spdx$/i.test(f));
  if (texts.length) return { texts: texts.map((f) => normalise(readFileSync(join(dir, f), "utf8"))) };
  if (files.length) {
    const parts = files.map((f) => spdxFallback(name, dir, f));
    return { note: parts.map((p) => p.note).join(" "), texts: parts.flatMap((p) => p.texts) };
  }
  return fail(`npm package ${name} ships no LICENSE file in ${dir}; add its text by hand`);
}

/** Licence notes and texts for every installed version of a package, without repeats. */
function packageTexts(p) {
  if (!p.paths?.length) fail(`pnpm listed npm package ${p.name} without an install path`);
  const found = p.paths.map((dir) => {
    if (!existsSync(dir)) fail(`npm package ${p.name} is not installed at ${dir}; run pnpm install`);
    return licenseTexts(p.name, dir);
  });
  const notes = [...new Set(found.map((f) => f.note).filter(Boolean))];
  return { notes, texts: [...new Set(found.flatMap((f) => f.texts))] };
}

/** Production npm packages with their licence texts, from `pnpm licenses list --prod --json`. */
function npmSection() {
  const r = mustRun("pnpm", ["licenses", "list", "--prod", "--json"], root);
  if (r.missing) fail("pnpm is not installed. Run: corepack enable");
  const pkgs = Object.entries(JSON.parse(r.out)).flatMap(([license, list]) =>
    list.map((p) => ({ ...p, license })),
  );
  const lines = [];
  for (const p of pkgs.toSorted((a, b) => byCodePoint(a.name, b.name))) {
    const home = p.homepage ? ` (${p.homepage})` : "";
    const { notes, texts } = packageTexts(p);
    lines.push(
      `### ${p.name} ${p.versions.join(", ")}`,
      "",
      `Licence: ${p.license}${home}`,
      "",
      ...notes.flatMap((n) => [n, ""]),
      ...texts.flatMap((t) => ["```text", t, "```", ""]),
    );
  }
  return lines.join("\n").trim();
}

function render() {
  return [
    "# Third-party licences (generated)",
    "",
    "Generated by `scripts/gen-licenses.mjs`. Do not edit. Run `pnpm licenses:gen` to refresh it.",
    "The C libraries linked into the app are described in `THIRD_PARTY_NOTICES.md`.",
    "",
    "## Rust crates",
    "",
    "Each licence text below lists the crates that use it.",
    "",
    rustSection(),
    "",
    "## npm packages (production)",
    "",
    "Each package's licence text is copied from the LICENSE file it ships.",
    "",
    npmSection(),
    "",
  ].join("\n");
}

const next = render();
if (check) {
  if (!existsSync(outFile) || readFileSync(outFile, "utf8") !== next) {
    fail("THIRD_PARTY_LICENSES.generated.md is stale. Run `pnpm licenses:gen` and commit it.");
  }
  console.log("gen-licenses: up to date");
} else {
  writeFileSync(outFile, next);
  console.log(`gen-licenses: wrote ${outFile}`);
}
