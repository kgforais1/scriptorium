# Plan: M2a Phase 4, the OCR job, commands, and UI

Last reviewed: 2026-11-23
Status: draft

This is the implementation plan for Phase 4 of
[`2026-11-07-m2a-ocr-pipeline.md`](2026-11-07-m2a-ocr-pipeline.md). The parent plan's "The OCR
job", "Engine interface" (engine input and memory budget), "Storage", "UI", "Word confidence",
and "Errors" sections hold the design. This plan fixes the order of work, the details those
sections leave open, and the checks for each chunk. Where the two disagree, this plan wins for
Phase 4, and the parent plan is corrected in the same PR.

## Goal

A book is OCRed in the background after ingest. The job runs every page through the bundled
Tesseract, writes the `ocr` layer of each page file, and keeps `ocr/index.json` current. The
reader shows progress and offers Stop, Resume, and "Recognise text again". A cancelled or crashed
run resumes without redoing finished pages. The word-confidence threshold M2c needs is measured
on a calibration set and recorded in `docs/ocr.md`. Showing the text is M2b, not this phase.

## Starting point

Phases 2 and 3 left these pieces, which this phase consumes:

- `src-tauri/src/ocr/engine.rs`: `OcrEngine`, `PageOcr`, `OcrError`, `normalise`.
- `src-tauri/src/ocr/fake.rs`: `FakeEngine`, test only. It returns one fixed result at once.
- `src-tauri/src/ocr/store.rs`: `read_page`, `OcrLayer::new` and `is_current`, `write_ocr_layer`,
  `read_index`, `write_index`, `read_forced`, and `rebuild_index`.
- `src-tauri/src/ocr/tesseract.rs`: `TesseractEngine::locate` and its `OcrEngine` impl. The
  empty-`langs` refusal is already tested (`no_language_is_refused_before_spawning`).
- `src-tauri/src/ocr/mod.rs`: a minimal `OcrUnavailable` struct, marked for replacement here.
- `src-tauri/src/library/load.rs`: `load_book`, which confines page paths (A12) and checks each
  display image's size and SHA-256 (A10).
- `src-tauri/src/library/sweep.rs`: `lock_ingest_dir` and `INGEST_LOCK`.
- `src-tauri/src/ingest/decode.rs`: `JP2_MAX_PIXELS`, inside the private `mod decode`. The
  colour type comes from `image::ImageReader`, so `header_info` is not needed.
- `src-tauri/src/commands/`: `create_and_open` (the ingest path), `throttle::EmitThrottle`,
  `session::canonical_key`, `view.rs` (which simplifies roots with `dunce`), and the `RunEvent`
  hook in `lib.rs`.
- Frontend: `src/lib/api.ts` (the `ingest-progress` listener pattern), `src/lib/errors.ts`, and
  `src/features/reader/ReaderHeader.tsx`, which holds the page indicator.

`src-tauri/src/lib.rs` declares `mod ocr;` privately, so a binary target cannot reach it.

## Decisions this plan adds

1. **OCR starts on its own after ingest, and only then.** This settles the parent's open
   question. OCR is offline and cancellable, so it starts as soon as `create_and_open` succeeds.
   Opening an existing book does not start a job. If it has unfinished pages, the reader shows
   Resume. A book OCRed by an older engine therefore waits for the curator, so an app update
   does not start work in every book a curator opens.
2. **Large JPEGs decode through `jpeg-decoder`.** The app decodes JPEG with `zune-jpeg`, through
   `image`, and `zune-jpeg` has no DCT scaling: its `set_max_width` is a limit, not a scale. So
   Chunk 2 adds `jpeg-decoder` 0.3 as a direct dependency, used only for a JPEG above the PDF
   caps. Display images are already 8-bit sRGB and orientation-applied, because ingest
   re-encodes CMYK, deep, non-sRGB, and rotated JPEGs. So the checks that matter are a large
   baseline JPEG and a large progressive JPEG, with both output dimensions within the caps, plus
   `cargo about` and `cargo audit`. If a check fails, the fallback treats a large JPEG like a
   PNG: a whole decode under the 200 MB rule, which refuses any 8-bit RGB JPEG above about 66 MP.
3. **Error kinds.** `BookError` gains `OcrUnavailable { reason }` and `OcrStorage { path, error }`,
   with their `CommandError` mapping in `library/command_error.rs` (its variant-count test goes
   from 13 to 15) and their messages in `src/lib/errors.ts`. The `OcrUnavailable` struct in
   `ocr/mod.rs` is removed, and `TesseractEngine::locate` returns the new kind. A write failure in
   `store.rs` becomes `OcrStorage`. A schema violation stays `InvalidPackage`, because it is a
   bug, not a disk problem. There is no `OcrAlreadyRunning` kind: `run_ocr` returns the running
   job's state.
4. **The pool is plain threads.** The app has no async runtime in its core and no `rayon`. The
   job registry, `OcrJobs`, owns a fixed set of `std::thread` workers, sized to half of
   `std::thread::available_parallelism`, at least 1 and at most 4. Workers take pages from the
   running books in turn, one page per book per turn. `OcrJobs` is Tauri-managed state beside the
   existing `Session`. It is built with an engine factory and the resource paths as arguments,
   never from environment variables, so tests inject both without touching the shared process
   environment.
5. **Lock order.** There are two kinds of lock: the registry lock, which guards the in-memory
   job states and `seq` counters, and one mutex per book, which guards a page file's
   read-change-write and `forced.json`. Code may take a per-book mutex and then the registry lock,
   never the reverse. Events are built under the registry lock and emitted after it is released.
6. **One lock helper and safe temp names.** `sweep.rs`'s `lock_ingest_dir` becomes
   `lock_dir(dir, name)`, which opens the lock file or creates it, as today. The constants are
   `INGEST_LOCK` (`.ingest.lock`), `JOB_LOCK` (`ocr/.job.lock`), and `RUN_LOCK` (`.ocr.lock`, in
   each temp run directory). `ocr/.job.lock` stays on disk between runs, so it must never be
   opened with `create_new`. The repo has no random-number crate, so temp names follow
   `library/create.rs`: the process id and the clock's nanoseconds. A run directory is made with
   `create_dir`, which fails if the name exists, and a clash retries with a new directory name.
   Only the fresh run directory's lock is opened with `create_new`, because nothing can exist
   there yet. The forced `runId` uses the same source.
7. **One deadline per page, and cancel reaches the input step.** A page's deadline is 120 seconds
   from when a worker takes it. It covers both the input step and recognition. The input step takes
   the stop flag and the deadline, and checks them before decoding, after decoding, and after
   resizing. A single decode call cannot be interrupted, so Stop can wait for the one decode in
   flight. The largest allowed decode is a 200 MB buffer, so Chunk 2 measures that wait and the
   test bounds it. The parent's "within about 100 ms" holds for recognition, and is corrected to
   say so.
8. **Temp files are deleted per page.** Each page's input PNG, TSV, and stderr file are deleted
   when the page ends, whether it succeeded, failed, or was cancelled. The run directory therefore
   holds at most one page's files per worker. A PNG at the 12 MP cap is under 48 MB, so a full pool
   of 4 needs under 200 MB of temp space at once.
9. **Events.** `ocr-progress` is `{ root, seq, done, failed, total }`, throttled with
   `EmitThrottle` at 250 ms, which gives the parent's 4 a second. The job passes `done + failed` as
   the throttle's count, so the last tick is always sent even when a page failed. `ocr-state` is
   `{ root, seq, state, reason }`, never throttled. `reason` is set only for `unavailable`, so a
   job started after ingest can still tell the reader why OCR is unavailable. The `ocr_status`
   snapshot carries the same `reason`. Both carry the root
   as `BookView.root` does, simplified with `dunce`, so the reader can compare them on Windows.
10. **Status lives in the reader header.** `ReaderStatus` is a full-page overlay for loading,
    empty, and error states, and returns nothing otherwise. OCR progress and "Text ready" are shown
    in `ReaderHeader`, beside its page indicator (`reader__indicator`) and the Text button, so they
    never cover the page.
11. **The pre-check image is bundled.** `tests/fixtures/ocr/smoke.png` (4 KB, one known line) is
    copied to `src-tauri/resources/ocr-check.png` and listed in `bundle.resources`. Debug builds and
    tests fall back to that file under `CARGO_MANIFEST_DIR`, as `TesseractEngine::locate` does for
    the binary. The pre-check requires the known words in the output, so a truncated
    `eng.traineddata` fails it.
12. **Dead code between chunks.** Chunks 1 to 4 add code whose production caller lands in
    Chunk 5. CI runs clippy with `-D warnings` on the library target, where test uses do not
    count. So each new module is declared with `#[allow(dead_code)]` and a comment naming the chunk
    that consumes it, as `ocr/mod.rs` does today. Chunk 5 removes every one of them, and the
    library clippy run proves nothing is left unused.
13. **`ocr_page` reaches the OCR code through a gated module.** `src-tauri/Cargo.toml` gains an
    empty `dev-tools` feature. `lib.rs` gains `#[cfg(feature = "dev-tools")] pub mod ocr_dev`,
    which exposes one function that runs an image through the input step and the Tesseract
    engine. `src-tauri/src/bin/ocr_page.rs` has `required-features = ["dev-tools"]`, so release
    builds never contain it. `ocr_spike.py` runs `cargo run --manifest-path src-tauri/Cargo.toml
    --features dev-tools --bin ocr_page`.
14. **The calibration image archive is a manual release.** The script packs the pages, and the
    archive is published as `ocr-calibration-v1` with `gh release create`, by hand. Creating a
    public release needs the user's approval, so Chunk 7 asks before publishing it.

## Chunks

Each chunk is one PR, reviewed and merged before the next starts.

### Chunk 1: errors, locks, and the temp run directory

- Add `OcrUnavailable` and `OcrStorage` (decision 3).
- Generalise the lock helper (decision 6).
- Add `ocr/run_dir.rs`: a `RunDir` guard that creates `scriptorium-ocr-run-<pid>-<nanos>` under
  `std::env::temp_dir()`, holds its `.ocr.lock`, and removes the directory on drop.
- Add the temp sweep: top level only, the `scriptorium-ocr-run-` prefix only, no symlinks, older
  than 1 hour, and only when the lock can be taken. `commands::setup` calls it at startup. The job
  calls it before each run from Chunk 3.
- Tests: a stale and a live directory, a directory without the prefix, a symlink with the prefix,
  and a name clash on create.
- Checks: `cargo test`, clippy on the library and all targets, and the existing ingest lock tests
  unchanged.

### Chunk 2: the engine-input step

- Change `JP2_MAX_PIXELS` in `ingest/decode.rs` from `pub(super)` to `pub(crate)`, and re-export
  it from `ingest/mod.rs` as `pub(crate) use decode::JP2_MAX_PIXELS as MAX_PAGE_PIXELS`, so the
  input step reuses ingest's 150 MP limit while `decode` stays private. The re-export lands here,
  with its first consumer, because an unused `pub(crate) use` fails clippy's `unused_imports`.
- Add `ocr/input.rs`. It reads the size and colour type with `image::ImageReader`, without
  decoding pixels. Bytes per pixel come from the colour type. It refuses a page over 150 MP, and a
  whole decode whose buffer would pass 200 MB. It decodes a page within the caps as it is, and a
  larger JPEG through `jpeg-decoder` at the smallest scale at or above the caps (decision 2). It
  resizes to the caps in one pass, never upscales, and writes a PNG into the run directory. It
  takes the stop flag and the deadline (decision 7).
- Add the large-page permit: one slot shared by all books. A page is small only when its decode
  buffer, from the header's size and bytes per pixel, is at most 48 MB. Every other page needs
  the permit. So a 12 MP RGBA16 page, which decodes to 96 MB, is a large page even though it is
  within the size caps.
- Add the counting allocator as a `#[cfg(test)] #[global_allocator]` in `lib.rs`, so it exists
  only in the unit-test binary, which can reach the private `ocr::input`. It counts only on a
  thread that has armed it. Each budget test arms its own thread after building its fixture, and
  the budget tests share one mutex, so they run one at a time. Other tests running in parallel do
  not count, because they never arm their threads.
- Tests on generated images: a 16 MP and a 100 MP JPEG, both baseline and progressive, a 16 MP
  PNG, a WebP page, a 60 MP RGBA PNG (240 MB, refused), a 60 MP RGB8 PNG (180 MB, accepted under the
  large-page permit), 50 MP and 20 MP 16-bit RGBA PNGs, and a header claiming
  200 MP. Each output is within the caps in both dimensions, and each peak is within its budget.
  Each refusal happens before any decode. The two-book permit test checks one holder at a time and
  a combined peak within 600 MB. A cancel during a 100 MP decode returns within the measured bound.
- Run `pnpm licenses:gen` and commit `THIRD_PARTY_LICENSES.generated.md`. Then `pnpm
  licenses:check` and `cargo audit` must pass.

### Chunk 3: the job core

- Add `ocr/job.rs` with `OcrJobs` (decisions 4 and 5). A job opens the book with `load_book`, so
  page paths stay confined, and takes each page's `imageSha256` from the checked `meta.json`.
  A `load_book` failure is passed on unchanged, so a checksum mismatch or a permission error keeps
  its own kind. Only an unreadable or invalid `meta.json` is `InvalidPackage`.
- The job rebuilds the index, then runs `pending` pages in order, then retries the pages that were
  `failed` when the run started, once each. A page that fails during this run is not retried until
  the next run. It skips a `pending` page whose layer `is_current`. The retry pass ignores that
  skip: a recorded failure wins over a current layer, so a page whose forced re-run failed is
  retried even though its older layer is still current. It writes each result with
  `write_ocr_layer` under the per-book mutex, keyed by `canonical_key`.
- It writes the index at most once a second, and at once after a page failure. When the job ends,
  whether it finished, was cancelled, or stopped on an error, it writes the index once more before
  the registry drops the live snapshot. So `book_status` never pairs the last `seq` with an older
  index. A failed write
  stops the job with `OcrStorage`, and the state becomes `unavailable`.
- The pre-check runs once per job, before any page (decision 11). It and the stale sweep run
  before the first page.
- Events and the in-memory snapshot (decision 9). Cancel sets the stop flag. Pages in flight are
  left `pending`, and their temp files are deleted (decision 8).
- Add `ocr/status.rs` with `book_status(root, jobs)`, the function `ocr_status` will call. While a
  job runs, it returns the registry's snapshot. After a job ends as `unavailable`, the registry
  keeps that state and its `reason` until the next run starts, and `book_status` returns them, so
  a reader that opens later still sees why. Otherwise it reads `index.json` as the parent's
  "`ocr_status`" and "Books with no v1 index" paragraphs describe: a different `engine` or a
  changed `imageSha256` makes a `done` page `pending`, a missing page file makes it `pending`, an
  existing `forced.json` keeps the forced pages unfinished, and a missing or v0 index makes every
  page `pending`.
- Extend `FakeEngine` into a scripted fake: a result per page index, an optional wait until the
  stop flag is set, and a log of calls. Its current one-result form stays as a constructor, so the
  store tests in `ocr/tests.rs` keep working.
- Tests: resume after cancel, cancel within a second with a page in flight, a cancelled retry
  that keeps its page `failed`, a failing page that is not
  retried in the same run, then retried once in the next, a second request joining the running job, two books sharing the
  pool, progress throttling and the last tick after a failure, `OcrUnavailable` for a missing or
  non-executable binary and for missing or truncated language data with no page attempted, the
  read-only `ocr/` folder mid-run, a progress event sent before the index flush with `book_status`
  returning that event's counts and `seq`, every `book_status` case above, and a reader that queries after an `unavailable` job
  ended gets the state and its reason. A page that finishes inside the one-second window just
  before the job ends, or before a cancel, is in the index on disk when the snapshot drops. A
  failed page whose layer is still current is retried on the next run.
- Test the Tesseract kill path with a stub binary that sleeps: both `stop` and the deadline end
  it, and no child is left.

### Chunk 4: forced runs and the cross-instance lock

- `run_ocr` with `force` writes `ocr/forced.json` atomically before its first page. A forced
  request during a run writes a new `runId` at once, and the running job adopts it and makes one
  more pass.
- The job holds `ocr/.job.lock` with `lock_dir`. An instance that cannot take it reports `busy`
  and writes nothing. A forced request there is refused with `busy`. `book_status` tries the lock
  without blocking and releases it at once.
- The second-instance tests re-run the test binary itself as a child process, with an environment
  variable that selects a helper mode. The helper takes the job lock and waits. The child gets its
  own environment, so the parent process's environment is untouched.
- Tests: a forced run redoes a fresh page and keeps a `skipped` one, resumes after a cancel on
  exactly the pages without its `runId`, and survives a lost index through `forced.json`. A
  failure survives a clock skew, because only `afterRunId` decides. While the helper holds the
  lock, the job writes nothing and reports `busy`. Once the helper exits or is killed, the next
  `book_status` call leaves `busy`.

### Chunk 5: commands and the reader

- Add `run_ocr(root, force)`, `cancel_ocr(root)`, and `ocr_status(root)` in a new
  `commands/ocr.rs`, and register them in `lib.rs`. Start a job after `create_and_open`
  (decision 1). On `RunEvent::Exit`, set every stop flag and wait up to 2 seconds.
- Remove every `#[allow(dead_code)]` added for Phase 4, the two in `ocr/mod.rs`, and the 11
  `cfg_attr(not(test), allow(dead_code))` markers in `ocr/store.rs`, whose items now have
  callers (decision 12). Keep a marker only where clippy shows the item is still unused, with a
  comment saying why.
- Frontend: `api.ts` gains the three calls and the two listeners. `src/lib/ocrStatus.ts` holds the
  event-ordering logic as a pure function, so Vitest's coverage gate on `src/lib/**` covers it. A
  thin `useOcrStatus` hook in `src/features/reader/` subscribes first, then queries, and applies
  only events with a higher `seq`. `ReaderHeader` shows "Reading text… 34 of 202", "Text ready",
  "Text ready, 1 page failed", or "Reading text in another window" (decision 10), and the reason
  when OCR is unavailable. `ReaderHeader` gains the Text button with "Recognise text again", Stop,
  and Resume, shown as the parent's UI section says.
- Frontend tests: each action, Resume hidden while `cancelling` or `busy`, Resume after
  `unavailable` calling `run_ocr` with `force: false`, the 5-second `busy` poll, and events
  replayed in each order around the query.
- Manual check: the built macOS app OCRs the 202-page Dulac book while the reader stays
  responsive. The PR records total time and peak memory. The app's buffers stay within the 600 MB
  step budget. The Tesseract children add their own working set, which the PR reports apart.

### Chunk 6: the `ocr_page` dev binary and the book scorer

- Add the `dev-tools` feature, `ocr_dev`, and `ocr_page` (decision 13). `ocr_page` prints the
  TSV words with their confidences as JSON.
- Add `--score-book <package>` to `scripts/ocr_spike.py`. It reads a book's `ocr/*.json` text for
  the ground-truth pages of that book and prints CER and WER. A fixture named
  `<book>-<NNNN>.txt` maps to the page whose source file has leaf number `NNNN`, found through
  `meta.json`, because page files are numbered from zero. The 12 pages span two books, so the
  script is run once per book. The Chunk 5 run is scored with it, and the
  result is recorded in `docs/ocr.md`.
- Checks: ruff, basedpyright, `cargo build --features dev-tools`, and a release `cargo build` that
  contains no `ocr_page`.

### Chunk 7: the calibration set and the threshold

- Write `scripts/fetch_calibration_set.py` as the parent's "Calibration set" describes: at least
  120 validated pages from at least 12 public-domain books, at least 5 pages from each, spread
  across the late 18th to early 20th century, with the content mix the parent lists. Commit
  `tests/fixtures/ocr-calibration/manifest.json`. Ask the user before publishing the image archive
  (decision 14).
- Add a manifest check, run in `Tests & coverage`: book and page counts, pages per book, the
  licence and source fields, and the shape of each SHA-1 and SHA-256.
- Extend `scripts/ocr_spike.py` with the alignment, running-head removal, candidate thresholds,
  and the F2 choice. Record the threshold, the per-book table, and the normalisation rules in
  `docs/ocr.md`. If any of the parent's four conditions holds, report to the user before M2c
  adopts the threshold.
- Checks: ruff, basedpyright, and unit tests for the alignment and running-head rules.

### Chunk 8: docs and close-out

- Update `docs/book-format.md`, `docs/development.md`, and both changelogs.
- Add the OCR weak-spots section to `docs/user/getting-started.md`, as the parent's last checklist
  item describes.
- Tick the parent's Phase 4 boxes and its Verification list, and archive this plan.

## Verification

- `cargo test` passes on Linux, macOS, and Windows in CI, including the real-engine test.
- Rust coverage stays at or above 80% lines. Frontend tests pass.
- The local Sonar scan reports no new issues before every commit and push.
- The built macOS app OCRs the 202-page Dulac book in the background while the reader stays
  responsive. The PR records total time, peak memory, and CER on the 12 ground-truth pages.
- Cancelling mid-run and starting again finishes the book without redoing finished pages.
- `pnpm licenses:check` passes, and the bundle carries the new notices.

## Open questions carried forward

The parent's language bundling, language manager, PaddleOCR, EasyOCR, help menu, and PDF word
box questions are follow-ups after M2a. None of them blocks Phase 4, which ships English only.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `jpeg-decoder` fails a check | low | med | Decision 2's fallback refuses large JPEGs instead. |
| The counting allocator misreads a peak | med | med | Per-thread arming and serial budget tests. The 100 MP test proves the scaled decode. |
| A deadlock between the registry and a book's mutex | low | high | Decision 5 fixes one order, and events are emitted outside the lock. |
| Stop waits on a large decode | med | low | Decision 7 bounds it by the largest allowed decode, and a test measures it. |
| Temp space runs out on a long book | low | med | Decision 8 deletes each page's files, so the run directory stays under 200 MB. |
| Exit waits too long on a slow page | low | low | The 2-second cap. Atomic writes make an early end safe. |
| Wikisource has too few validated pages that fit | med | med | The script reports the count per book before any page is scored. The set may take more books. |
