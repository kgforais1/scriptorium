# Plan: M2a OCR engine and pipeline

Last reviewed: 2026-11-07
Date: 2026-11-07
Author: planning pass (branch `plan/m2-ocr`)
Status: draft
Linked issue/PR: n/a
Roadmap: [M2 — OCR-aware scans](2026-08-23-product-roadmap.md#m2--ocr-aware-scans-text-under-the-image)

## Goal

Give every page of a book machine-readable text with word and line boxes, produced offline by
an engine that ships with the app. OCR runs in the background after ingest and on request. It
never overwrites text a curator has supplied. This is the foundation the rest of M2 reads from:
search, the hidden text layer, the correction editor, and text export all consume the files
this plan writes.

## How M2 is split

M2 is too large for one plan. It ships as four plans, in this order:

| Plan | Scope | Depends on |
|---|---|---|
| **M2a (this plan)** | Engine choice, bundling, OCR job, `ocr/` schema v1 with boxes, precedence rules, status | M1.5 |
| M2b | Full-text search UI, jump and highlight, hidden text layer for select and copy | M2a |
| M2c | Curator correction editor, curator-supplied text, correction history, "reviewed" mark | M2a |
| M2d | Page export, with the page text embedded in exported JPEG, PNG, and PDF files | M2a |

M2b and M2c can run in parallel once M2a merges. The roadmap's M2 checklist maps onto them as
listed. Each later plan is written when M2a's schema has settled.

## Out of scope

- Search, the text layer, and any UI that shows page text. Those are M2b.
- Editing text, and curator-supplied text. M2c adds the `curator` layer. M2a only reserves its
  place in the schema and proves OCR never touches it.
- Export with embedded text. That is M2d.
- Word boxes for the text a PDF carries. M2a OCRs PDF pages like any other page. Mapping
  pdfium's character boxes through the render transform is a follow-up, recorded below.
- Languages other than English. Both test corpora are English. The engine interface takes a
  language list, so adding a language later is a data file, not a code change.
- A cloud OCR engine, or PaddleOCR. The interface allows one later.
- Handwriting recognition.
- A `formatVersion` bump. `ocr/` is outside `meta.json` (decided 2026-10-29).

## Approach

### Engine choice: a spike, then a gate

The roadmap names Tesseract as a sidecar as the lead candidate and asks for a quality check
first. Phase 1 compares two engines on the test corpora and on packaging cost:

- **Tesseract 5** as a sidecar binary (`bundle.externalBin`), with `eng.traineddata`. Its TSV
  output already gives block, paragraph, line, and word boxes with a confidence each. Running it
  as a separate process means a crash on a bad page cannot take down the app.
- **ocrs**, a pure-Rust engine (MIT/Apache-2.0) that compiles into the app. Its models are two
  files of about 10 MB each, fetched and pinned like pdfium. It has no native dependencies, so
  packaging is trivial, but it is younger and its layout analysis is weaker.

The spike measures, for each engine:

1. **Accuracy.** Character error rate (CER) and word error rate against a hand-checked ground
   truth of 12 pages: 6 from Dulac and 6 from Princess and Goblin, chosen to include a plain text
   page, a caption under a plate, a title page, a page with a drop cap, a page with foxing, and
   a plate with no text. Ground truth goes in `tests/fixtures/ocr/` as text files. The page
   images stay in gitignored `tmp/`, because they are large. The fixture README records how to
   rebuild them.
2. **Speed.** Seconds per page on an 11 MP display image, on the dev Mac, single-threaded.
3. **Packaging.** Whether a self-contained binary or crate runs on a clean macOS arm64 machine
   with no Homebrew, and the recorded route for Windows x64 and Linux x64. For Tesseract this
   means a build with static Leptonica and image libraries, produced by a pinned script in CI.
4. **Size** added to the installer.

CER and WER are computed over the whole set: total edits divided by total ground-truth
characters or words. The blank plate is scored separately, by how many characters the engine
invents on it.

The gate: pick Tesseract unless its packaging route fails on any of macOS arm64, Windows x64, or
Linux x64, or ocrs is within 1
percentage point of Tesseract's CER at less than half its added size. The spike result and the
choice are recorded in this plan and in `docs/ocr.md` before Phase 2 starts. If neither engine
reaches 3% CER on the plain text pages, stop and revisit the plan with the user.

The rest of this plan is written for Tesseract. Where ocrs would differ, the difference is noted
in brackets.

### Engine interface

```rust
pub trait OcrEngine: Send + Sync {
    /// Stable id and version, recorded with every result, e.g. `tesseract 5.5.1 eng-fast`.
    fn id(&self) -> EngineId;
    /// Returns `OcrError::Cancelled` soon after `stop` is set, and `OcrError::Timeout`
    /// once `deadline` passes.
    fn recognize(
        &self,
        image: &Path,
        langs: &[Lang],
        stop: &AtomicBool,
        deadline: Instant,
    ) -> Result<PageOcr, OcrError>;
}
```

`PageOcr` holds the pixel width and height of the image the engine read, and lines, each with
a box and its words, each word with a box, text, and a confidence from 0 to 1. Boxes are in
pixels of that image. The job, not the engine,
normalises them to 0–1 by the display image's size. A `FakeEngine` in tests returns fixed
results, so most tests need no engine binary.

The Tesseract engine runs the sidecar as `tesseract <input> <tmpbase> -l eng --psm 3 --oem 1 tsv`,
which writes `<tmpbase>.tsv`. Output goes to a file, not a pipe, and stderr goes to a capped temp
file, so a full pipe buffer can never stall the child. The engine reads the TSV after the child
exits and parses it. The page row gives the image size. It drops rows with an empty text or a confidence of -1, and
groups words into lines by `block_num`, `par_num`, and `line_num`. It spawns the child, then
polls `try_wait` every 100 ms. It kills the child when `stop` is set or the 120-second deadline
passes, then reaps it. Tesseract starts no child processes of its own, so killing the one process
is enough on every platform. The child runs with `OMP_THREAD_LIMIT=1`, so the job's pool, not
Tesseract, decides how many cores are busy. [ocrs: an in-process call. It checks `stop` and the
deadline between its detection and recognition stages, which is the finest grain the crate
allows. A page that overruns is therefore bounded by one stage, not abandoned to run on.]

OCR reads the display image, not the original. Boxes therefore match what the reader shows, with
no transform. Only PDF pages are capped at ingest. An image page keeps its full size, and may be
WebP, which a static Tesseract may not read. So the job prepares the engine's input: it decodes
the display image, scales it down to the PDF caps (4000 px long edge, 12 MP) if it is larger, and
writes a PNG to a temp file. Engine inputs and outputs live in a per-run directory under
`std::env::temp_dir()`, held by a guard that removes it on drop, so nothing temporary lands in the
book. The existing sweep only scans a book's top level, so it would not have caught files in
`ocr/` anyway. Boxes are normalised by the size of the image the engine saw,
so scaling does not move them. A test runs a 16 MP TIFF page and a WebP page through this step.

### Storage: `ocr/` schema v1

`ocr/<NNNN>.json` holds one page. It keeps each source's text in its own layer:

```json
{
  "schema": 1,
  "layers": {
    "ocr": {
      "engine": "tesseract 5.5.1 eng-fast",
      "generatedAt": "2026-11-07T12:00:00Z",
      "imageSha256": "…",
      "text": "…",
      "lines": [
        { "bbox": [0.1, 0.2, 0.6, 0.03],
          "words": [ { "text": "Once", "bbox": [0.1, 0.2, 0.05, 0.03], "conf": 0.96 } ] }
      ]
    },
    "pdf": { "text": "…" }
  }
}
```

- `bbox` is `[x, y, w, h]` in normalised page coordinates, origin top-left of the display image,
  as `docs/book-format.md` already reserves. Values are rounded to 5 decimal places, which is
  under a tenth of a pixel at 4000 px.
- `text` in the `ocr` layer is the lines joined with `\n`, and paragraphs with `\n\n`. It is
  stored rather than derived so search in M2b can read one field.
- `imageSha256` is the page's `sha256` from `meta.json` at the time of the run. A page whose
  checksum has changed is stale and is re-run.
- `pdf` holds the text M1.5 extracted. It has no boxes in M2a.
- `curator` is reserved for M2c. The v1 schema lists it with its M2c shape left open
  (`additionalProperties` allowed inside it). M2a never writes it and never removes it.
- The published text of a page is the first layer present in the order `curator`, `ocr`,
  `pdf`. That rule lives in one Rust function and is documented in `book-format.md`. `ocr` beats
  `pdf` because the text layer in M2b needs the published text and its boxes to come from the
  same transcript, and the `pdf` layer has no boxes. A PDF page's text is therefore published
  only until OCR has run on it. The `pdf` layer is kept, so M2b can still search it, and the
  follow-up below can give it boxes and move it ahead of `ocr` (decided 2026-11-07).

`ocr/index.json` v1:

```json
{
  "schema": 1,
  "engine": "tesseract 5.5.1 eng-fast",
  "updatedAt": "…",
  "pages": { "0": { "status": "done" }, "1": { "status": "failed", "error": "timeout" } },
  "summary": { "pages": 202, "done": 201, "failed": 1, "pending": 0, "withText": 180 }
}
```

`status` is one of `pending`, `done`, `failed`, or `skipped`. A plate with no text is `done`
with an empty `text` and no lines. `skipped` is reserved for M2c ("curator says do not OCR").
`pages` lists every page in the book, not only PDF pages.

**Reading v0.** M1.5 writes v0, which has no `schema` field. A file is read as v0 only if it
validates against `ocr.v0.schema.json`. A file that is neither valid v0 nor valid v1 is invalid,
and the rule under Errors applies. The loader turns a valid v0 page file into a v1 file with only
a `pdf` layer. That layer needs no checksum, because only the `ocr` layer is ever checked for
staleness. It writes v1 the next
time the job touches the page. v0 has never shipped in a release, so this is a small, tested
shim, not a migration. M1.5's ingest switches to writing v1 in Phase 2, and `ocr.v0.schema.json`
is kept for the shim's tests.

**Writing.** Today `library/ocr.rs` writes with plain `std::fs::write`, which is not crash-safe.
Phase 2 switches it to the existing `write_atomic` in `library/mod.rs`, so a crash leaves the old
file or the new one. Every `ocr/` write goes through it.

**The index is a cache.** The page files are the truth. The job writes the index at most once per
second, so after a crash it can lag. The job therefore rebuilds the index before it starts. It
walks `0..meta.pages.len()` from `meta.json` and compares each page file against that page's
`sha256`: each page is `done` if its `ocr` layer is fresh (same engine id and same
`imageSha256`), `pending` if it is missing or stale, and `failed` if the file is invalid. A
`failed` status from a timeout is not on disk, so it becomes `pending` and is retried once per run.
A `skipped` status is kept from the old index. M2c moves it into the page file, where it belongs.

**Books with no v1 index.** `ocr_status` treats a missing `index.json`, which every M1 image book
has, as every page `pending`. It treats a valid v0 index as `pending` for every page, with the
v0 `withText` count as a hint. It never fails on either. The next job writes a v1 index.
Reading 202 small JSON files takes milliseconds. `ocr_status` trusts the index as written, so it
stays cheap, and may lag a crashed run until the next job starts. A test kills the job after a
few pages, and checks that the rebuilt index matches the files.

**Guarantee.** The job writes only the `ocr` layer of a page file. It reads the file, replaces
`layers.ocr`, and writes it back. It never rewrites a file it cannot parse and validate, so a
damaged file that still holds curator text is left byte-for-byte as it is. Two tests check this.
OCR over a page with `pdf` and `curator` layers leaves both layers identical. OCR over an invalid
file leaves the file unchanged and marks the page `failed`.

### The OCR job

- One job runs per book at a time. A second request for the same book returns the running job.
- All jobs share one global pool, sized to half the logical cores, at least 1 and at most 4. So
  two books OCR at once never use more than that. Each worker runs one page at a time. Books take
  turns page by page. [ocrs: same pool.]
- It starts automatically after a successful ingest, and from a new `run_ocr` command. A new
  `cancel_ocr` command sets the stop flag. Pages in flight end within about 100 ms and are
  left `pending`. On exit, `RunEvent::Exit` sets every stop flag and waits up to 2 seconds for
  the workers, then lets the process end. Atomic writes make an end mid-page safe. The next
  start resumes, because finished pages are on disk.
- It visits pages in order and skips a page whose `ocr` layer has the same engine id and the same
  `imageSha256` as now. So a re-run after an engine upgrade redoes everything, and a re-run after
  nothing changed is instant.
- It reports progress with a global `ocr-progress` event, as ingest emits `ingest-progress`:
  `{ root, done, failed, total }`, throttled to 4 a second with the existing throttle. An event,
  not a channel, because a job started after ingest has no command in flight to carry one.
- A page failure (timeout, a crash, unreadable output) marks that page `failed` with a short
  error and moves on. The job fails as a whole only if the engine cannot start at all, which is
  reported as `OcrUnavailable`, like `PdfiumUnavailable`.
- There is no per-book write lock today. M2a adds one: a mutex per book in the job
  registry, keyed by the canonical package root like `commands/session.rs`, never by the
  book's self-declared id. It is held only while a page file is read, changed, and written back.
  The job registry uses the same key. M2c's editor takes
  the same mutex. Reading the book while OCR runs is unaffected. `load_book` still does not read `ocr/`. It gains a cheap
  `ocr_status` read for the UI, which reads only `index.json`.

### UI

M2a adds only status, since showing text is M2b:

- The home screen has no book list until M5, so status shows in the reader. `ReaderStatus`
  shows "Reading text… 34 of 202" while the job runs, and "Text ready" or "Text ready, 1 page
  failed" after.
- `ReaderHeader` gets a "Text" button with two actions: "Recognise text again", which calls
  `run_ocr`, and "Stop", which calls `cancel_ocr`.
- Errors map through `src/lib/errors.ts` like the existing kinds.

### Bundling Tesseract

- `scripts/build_tesseract.sh` builds Tesseract and Leptonica from pinned source tarballs with
  pinned SHA-256 values, with static zlib, libpng, libjpeg-turbo, and libtiff, and no network
  features. It also links static libwebp, so WebP pages work even without the job's PNG step.
  It outputs one binary named for Tauri's sidecar convention, `tesseract-<target-triple>`, with
  `.exe` on Windows, into `src-tauri/binaries/`, which is gitignored. That suffixed name is also
  what debug builds and tests look for. CI builds it for each platform and uploads it as a pinned
  release asset on this repo, because a full build takes too long to run in every CI job.
- `scripts/fetch_tesseract.sh` downloads that asset and `eng.traineddata`, checks both
  hashes, and installs them the way `fetch_pdfium.sh` does, with the same `--proto =https`
  curl flags and `.version` early exit.
- `tauri.conf.json` gets `bundle.externalBin: ["binaries/tesseract"]` and a resource entry for
  `tessdata/eng.traineddata`, found through `app.path().resource_dir()` as pdfium's library is.
  That is `Contents/Resources/` on macOS. Tauri strips the triple and places the sidecar beside the main
  executable, which is `Contents/MacOS/` in a macOS bundle. Rust finds it from
  `std::env::current_exe()`'s directory, and spawns it with `std::process::Command` with fixed
  arguments plus the image path. In debug builds and tests it falls back to
  `src-tauri/binaries/` and `src-tauri/tessdata/`, as `bind_pdfium` falls back to
  `src-tauri/pdfium/`. A `SCRIPTORIUM_TESSERACT` variable overrides the binary path for tests. No
  shell plugin and no frontend capability are added, so the WebView cannot start it.
- `TESSDATA_PREFIX` is set, for the child only, to the directory that holds `eng.traineddata`.
  Tesseract 5 reads it as the tessdata directory itself. The integration test pins this.
- macOS: Tauri signs external binaries with the app. Phase 3 checks the built app runs the
  sidecar under the hardened runtime.
- Licences: Tesseract (Apache-2.0), Leptonica (BSD-2-Clause), libpng, libjpeg-turbo, libtiff,
  zlib, and the traineddata (Apache-2.0) go in `THIRD_PARTY_NOTICES.md`.
- The bump policy in `docs/ci-and-hooks.md` gains a Tesseract entry like pdfium's.
  Tesseract, Leptonica, and the image libraries have NVD CPEs, unlike pdfium. Phase 3 checks
  whether `check_bundled_c_cves.py` can track entries that are not crates. If it can, they go in
  `ci/bundled-c.toml`. If not, the bump policy lists them for a manual check at each bump.

[ocrs: no sidecar and no build script. `scripts/fetch_ocr_models.sh` fetches the two model files,
which are bundled as resources. The crate's licence goes through the existing cargo licence
check.]

### Errors

| Kind | When |
|---|---|
| `OcrUnavailable` | The engine binary or language data is missing or will not start. |
| `OcrAlreadyRunning` | Not an error to the UI; `run_ocr` returns the running job's id. |
| `InvalidPackage` | `meta.json` cannot be read, so the job cannot list the pages. |
| (page status) | An `ocr/<NNNN>.json` fails validation. This is not an error. The job leaves the file untouched and marks the page `failed` with the reason. M2c adds the repair action. |

Per-page failures are statuses, not errors.

### Alternatives considered

- **Tesseract linked in-process** (`leptess` or `tesseract-sys`). It avoids process start-up,
  which is about 0.2 s against several seconds of recognition. It loses crash isolation and
  still needs the same native build. Rejected.
- **tesseract.js in the WebView.** It packages easily, but runs in the UI process, is several
  times slower than native, and would split the pipeline between Rust and TypeScript. Rejected.
- **Platform OCR** (macOS Vision, Windows.Media.Ocr). Good quality and zero size, but three
  engines with different boxes and no Linux engine. Rejected for the default. A future engine
  behind the same trait could use it.
- **OCR the original, not the display image.** Slightly more pixels for small text, but every box
  would need the original-to-display transform, and originals can be TIFF or JPEG 2000. Rejected.

## Proposed file changes

- `plans/2026-11-07-m2a-ocr-pipeline.md` (this plan), `to_do.md`, `plans/README.md`.
- `docs/ocr.md` (new): engine choice, spike results, how to bump the engine.
- `docs/ocr.v1.schema.json` (new). `docs/book-format.md`: the v1 layout and precedence rule.
- `tests/fixtures/ocr/*.txt` (ground truth) and `tests/fixtures/README.md`.
- `scripts/ocr_spike.py` (Phase 1 only, kept for re-running the comparison on a bump).
- `scripts/build_tesseract.sh`, `scripts/fetch_tesseract.sh`, `.github/workflows/tesseract.yml`.
- `src-tauri/src/ocr/{mod.rs, engine.rs, tesseract.rs, fake.rs, job.rs, store.rs}` (new).
- `src-tauri/src/library/ocr.rs`: write v1, keep v0 reading for the shim.
- `src-tauri/src/commands/`: `run_ocr`, `cancel_ocr`, `ocr_status`.
- `src-tauri/src/lib.rs`: register the commands, cancel jobs on exit.
- `src-tauri/tauri.conf.json`: the sidecar and its language data.
- `src/features/reader/` (`ReaderStatus`, `ReaderHeader`): status and the Text button. `src/lib/errors.ts`.
- `THIRD_PARTY_NOTICES.md`, `docs/ci-and-hooks.md`, `docs/development.md`, both changelogs.

## Phases & checklist

### Phase 0: register the plan

- [x] Add this plan to `to_do.md` (Next Up and Active plans) and `plans/README.md`.
- [x] Record the M2 split in the roadmap under M2.

### Phase 1: engine spike and decision

- [ ] Pick the 12 ground-truth pages and transcribe them. Check each by eye against the scan.
- [ ] Write `scripts/ocr_spike.py`. It runs each engine on the 12 pages and prints CER, WER, and
      seconds per page. It must pass ruff and basedpyright.
- [ ] Run Tesseract with `tessdata_fast` and with `tessdata_best`, and ocrs.
- [ ] Prove the packaging route for the leading engine on macOS arm64: a binary that runs with
      Homebrew moved aside, signed ad hoc under the hardened runtime, inside a built app. Build
      the Linux x64 and Windows x64 binaries in CI, and run each on a runner with no Tesseract
      installed. A failure on any platform fails the gate for Tesseract.
- [ ] Apply the gate, write `docs/ocr.md`, and update this plan. Get the choice reviewed before
      Phase 2.

### Phase 2: schema, store, and engine interface

- [ ] Write `docs/ocr.v1.schema.json` with schema tests: valid page, valid index, each refusal.
- [ ] Write `ocr/store.rs`: read v0 and v1, write v1, replace only the `ocr` layer, the
      published-text rule, and the index recovery rules.
- [ ] Switch M1.5 ingest to write v1 page files with a `pdf` layer, and a v1 index.
- [ ] Add the `OcrEngine` trait, `PageOcr`, normalisation, and `FakeEngine`.
- [ ] Switch `library/ocr.rs` to `write_atomic`.
- [ ] Tests: both layer guarantees, v0 reading, a file that is neither v0 nor v1, stale
      detection by checksum and by engine id, box normalisation at both edges, and the index
      rebuild after a simulated crash.

### Phase 3: Tesseract engine and bundling

- [ ] Write `build_tesseract.sh` and the workflow that publishes the pinned binaries.
- [ ] Write `fetch_tesseract.sh`, and run it in the Rust CI jobs as pdfium is fetched.
- [ ] Write `ocr/tesseract.rs`: spawn, timeout, TSV parsing. Unit-test the parser on recorded TSV,
      including empty rows, `-1` confidences, and a multi-block page.
- [ ] Add one integration test that runs the real binary on a small committed fixture image. It
      runs in CI on every platform.
- [ ] Add the sidecar, resource, and capability config. Run `pnpm tauri build` on macOS and check
      the built app recognises a page. Add Windows and Linux bundle checks to the M1 follow-ups
      manual list.
- [ ] Add the notices and the bump policy entry.

### Phase 4: job, commands, and UI

- [ ] Write `ocr/job.rs`: pool, ordering, skip rule, progress, cancel, and per-page failure.
- [ ] Add `run_ocr`, `cancel_ocr`, and `ocr_status`, and start a job after ingest.
- [ ] Write the engine-input step: decode, scale to the caps, write a temp PNG. Test it on a
      16 MP TIFF and a WebP page.
- [ ] Tests with `FakeEngine`: resume after cancel, cancel returning within a second while a page
      is in flight, a failing page, a second request joining the running job, two books sharing
      the pool, progress throttling, and cancel on exit.
- [ ] Test that `run_ocr` returns `OcrUnavailable` when `SCRIPTORIUM_TESSERACT` points at a
      missing file, and at a file that is not executable.
- [ ] Test the Tesseract engine's kill path with a stub binary that sleeps: both `stop` and the
      deadline end it, and no child is left running.
- [ ] Add the home-screen status, the reader menu items, and the error mapping, with frontend
      tests.
- [ ] Update `book-format.md`, `development.md`, user docs, and both changelogs.

## Verification

- [ ] `cargo test` passes on Linux, macOS, and Windows in CI, including the real-engine test.
- [ ] Rust coverage stays at or above 80% lines. Frontend tests pass.
- [ ] The local Sonar scan reports no new issues.
- [ ] The built macOS app OCRs the 202-page Dulac book in the background while the reader stays
      responsive. The PR records total time, peak memory, and CER on the 12 ground-truth pages.
- [ ] Cancelling mid-run and starting again finishes the book without redoing finished pages.
- [ ] `pnpm licenses:check` passes, and the new notices are in the bundle.

## Open questions

- [x] Should embedded PDF text beat OCR text? Decided 2026-11-07: no, `ocr` beats `pdf` until
      PDF text has boxes, so the text layer matches its own boxes.
- [ ] `tessdata_fast` or `tessdata_best`? The spike decides. `best` is about 4 times larger and
      slower, for a small accuracy gain on clean print.
- [ ] Should OCR start automatically after ingest, or wait for the curator? The recommendation is
      automatic, because it is offline and cancellable.
- [ ] Follow-up: word boxes for embedded PDF text, from pdfium's character boxes. Only needed if
      OCR boxes prove a poor fit for PDF text in M2b.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| A static, relocatable Tesseract build is hard on one platform | med | high | Phase 1 proves macOS first. ocrs is the fallback behind the same trait. |
| OCR quality on old type is poor | med | med | The spike measures it before any building. M2c's corrections cover the rest. |
| OCR of a long book takes too long | med | low | It runs in the background, resumes, and never blocks reading. |
| A bad page hangs or crashes the engine | low | med | The sidecar has a timeout and is killed. The page is marked `failed` and retried once on the next run. |
| A damaged page file holds curator text | low | high | The job never rewrites a file it cannot validate. |
| The job and a later curator edit race on one page file | low | high | M2a never writes `curator`. Both take the per-book mutex for each read-change-write. |
| Installer size grows | high | low | Tesseract plus `eng` fast data is about 10 MB. The spike records the number. |
| Tesseract writes a large TSV or stderr and stalls on a full pipe | med | med | Output goes to files, never undrained pipes. |
| Sidecar spawning widens the app's attack surface | low | med | Only Rust spawns it, with fixed arguments. The WebView gets no shell access. |
