/* global window, HTMLInputElement -- these functions run inside the browser page. */
// Drives the real Reader in headless Chrome and takes screenshots. It runs the Vite dev server
// with a stubbed Tauri bridge, and opens `/`: `last_book` returns a book built from a folder of scans, and
// page files are served through Vite's /@fs/ route. Nothing touches the library or Rust, so
// the production CSP and asset scope are not covered (see docs/manual-test-m1.md section 8).
//
//   pnpm shots                     # Dulac corpus if present, else the 12-page fixture book
//   pnpm shots --pages <dir>       # any folder of JPEG or PNG scans inside tmp/
//   pnpm shots --save              # also copy the README set into docs/screenshots/
//
// First run: `pnpm exec playwright install chromium`.

import { copyFileSync, mkdirSync, openSync, readSync, closeSync, readdirSync, existsSync, realpathSync } from "node:fs";
import { isAbsolute, join, relative, resolve } from "node:path";
import { parseArgs } from "node:util";
import { chromium } from "playwright";
import { createServer } from "vite";

const ROOT = resolve(import.meta.dirname, "..");
const DULAC = join(ROOT, "tmp/fixtures/corpus/dulac-picture-book/jpg");
const FIXTURE = join(ROOT, "tmp/fixtures/fixture-book/pages");
const OUT = join(ROOT, "tmp/shots");
const SAVED = join(ROOT, "docs/screenshots");
const VIEWPORT = { width: 1280, height: 820 };
// The shots copied by --save, picked for the README.
const README_SET = ["reader-spread", "reader-turn", "reader-notes", "reader-scrub"];

const { values: args } = parseArgs({
  options: { pages: { type: "string" }, save: { type: "boolean", default: false } },
});

// Reads width and height from a PNG IHDR or a JPEG SOF marker, without decoding the image.
function imageSize(path) {
  const fd = openSync(path, "r");
  try {
    const head = Buffer.alloc(65536);
    const n = readSync(fd, head, 0, head.length, 0);
    if (head.readUInt32BE(0) === 0x89504e47) return { width: head.readUInt32BE(16), height: head.readUInt32BE(20) };
    return jpegSize(head.subarray(0, n), path);
  } finally {
    closeSync(fd);
  }
}

function jpegSize(buf, path) {
  let i = 2;
  while (i + 9 < buf.length) {
    const marker = buf[i + 1];
    const isSof = marker >= 0xc0 && marker <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(marker);
    if (isSof) return { width: buf.readUInt16BE(i + 7), height: buf.readUInt16BE(i + 5) };
    i += 2 + buf.readUInt16BE(i + 2);
  }
  throw new Error(`${path}: no JPEG size marker in the first 64 KB`);
}

// The scans folder, resolved through symlinks. It must sit inside tmp/: the scans belong with the
// fixtures, and Vite serves /@fs/ only inside the project.
function pagesDir(input) {
  const tmp = realpathSync(join(ROOT, "tmp"));
  const candidate = resolve(ROOT, input ?? (existsSync(DULAC) ? DULAC : FIXTURE));
  if (!existsSync(candidate)) throw new Error(`--pages folder does not exist: ${candidate}`);
  const dir = realpathSync(candidate);
  const rel = relative(tmp, dir);
  if (rel === "" || rel.startsWith("..") || isAbsolute(rel)) throw new Error(`--pages must be a folder inside tmp/: ${dir}`);
  return dir;
}

function loadBook(dir) {
  const files = readdirSync(dir).filter((f) => /\.(jpe?g|png)$/i.test(f)).sort();
  if (files.length === 0) throw new Error(`${dir}: no JPEG or PNG scans`);
  const pages = files.map((f) => ({ path: join(dir, f), ...imageSize(join(dir, f)) }));
  const title = dir.includes("dulac") ? "Edmund Dulac's Picture-Book" : "Fixture book";
  return { id: "shots", title, root: "/shots", lastReadPage: 0, saveSeq: 0, pages };
}

// Runs in the page before the app: a minimal stand-in for Tauri's IPC and event bridge.
function stubTauri(book) {
  let next = 1;
  window.__saves = [];
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => undefined };
  window.__TAURI_INTERNALS__ = {
    transformCallback: (cb) => {
      const id = next++;
      window[`_${id}`] = cb;
      return id;
    },
    convertFileSrc: (p) => `/@fs${p}`,
    invoke: async (cmd, a) => {
      if (cmd === "last_book") return book;
      if (cmd === "save_progress") {
        window.__saves.push(a.page);
        return "saved";
      }
      if (cmd === "close_book") return "saved";
      if (cmd.startsWith("plugin:event|")) return next++;
      throw Object.assign(new Error(`stub: no ${cmd}`), { kind: "Internal" });
    },
  };
}

async function openReader(browser, url, book, options = {}) {
  const ctx = await browser.newContext({
    viewport: options.viewport ?? VIEWPORT,
    reducedMotion: options.reduced ? "reduce" : "no-preference",
  });
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.addInitScript(stubTauri, { ...book, lastReadPage: options.lastReadPage ?? 0 });
  // The stubbed last_book makes App open the reader directly.
  await page.goto(url);
  await page.waitForSelector(".reader__canvas[data-ready=true]", { timeout: 30000 });
  await page.waitForTimeout(800);
  return { ctx, page, errors };
}

async function shot(page, name) {
  await page.screenshot({ path: join(OUT, `${name}.jpg`), type: "jpeg", quality: 85 });
  const indicator = await page.locator(".reader__indicator").textContent();
  console.log(`${name}: ${indicator}`);
}

async function scrubTo(page, spread) {
  await page.locator(".reader__scrubber").evaluate((el, value) => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    set.call(el, String(value));
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }, spread);
}

// The main tour: rest, mid-turn, notes, scrubbing, last page. Returns the pages saved.
async function tour(browser, url, book) {
  const start = Math.min(20, book.pages.length - 1);
  const { ctx, page, errors } = await openReader(browser, url, book, { lastReadPage: start });
  await shot(page, "reader-spread");
  await page.keyboard.press("ArrowRight");
  await page.waitForTimeout(450);
  await shot(page, "reader-turn");
  await page.waitForTimeout(1500);
  await page.getByRole("button", { name: "Notes", exact: true }).click();
  await page.waitForTimeout(600);
  await shot(page, "reader-notes");
  await page.keyboard.press("Escape");
  await page.locator(".reader__scrubber").focus();
  await scrubTo(page, Math.floor(book.pages.length / 4));
  await page.waitForTimeout(350);
  await shot(page, "reader-scrub");
  await page.waitForTimeout(2500);
  await page.keyboard.press("End");
  await page.waitForTimeout(3000);
  await shot(page, "reader-end");
  const saves = await page.evaluate(() => window.__saves);
  await ctx.close();
  return { saves, errors };
}

async function smallAndReduced(browser, url, book) {
  const small = await openReader(browser, url, book, { viewport: { width: 900, height: 520 }, lastReadPage: 9 });
  await shot(small.page, "reader-small-window");
  await small.ctx.close();
  const reduced = await openReader(browser, url, book, { reduced: true, lastReadPage: 9 });
  await reduced.page.keyboard.press("ArrowRight");
  await reduced.page.waitForTimeout(150);
  await shot(reduced.page, "reader-reduced-motion");
  await reduced.ctx.close();
  return [...small.errors, ...reduced.errors];
}

async function main() {
  const dir = pagesDir(args.pages);
  const book = loadBook(dir);
  mkdirSync(OUT, { recursive: true });
  const server = await createServer({ root: ROOT, logLevel: "error", server: { port: 1430, strictPort: false } });
  await server.listen();
  const url = server.resolvedUrls.local[0];
  const browser = await chromium.launch({ args: ["--use-angle=metal", "--ignore-gpu-blocklist"] });
  try {
    const { saves, errors } = await tour(browser, url, book);
    errors.push(...(await smallAndReduced(browser, url, book)));
    console.log(`saved pages: ${saves.join(", ")}`);
    console.log(errors.length ? `page errors:\n  ${errors.join("\n  ")}` : "page errors: none");
  } finally {
    await browser.close();
    await server.close();
  }
  if (args.save) {
    mkdirSync(SAVED, { recursive: true });
    for (const name of README_SET) copyFileSync(join(OUT, `${name}.jpg`), join(SAVED, `${name}.jpg`));
    console.log(`copied ${README_SET.length} shots to docs/screenshots/`);
  }
  console.log(`shots in ${OUT}`);
}

await main();
