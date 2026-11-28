# Plan: M2a Phase 4, the OCR job, commands, and UI

Last reviewed: 2026-11-27
Status: in-progress

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

Chunk 1 replaced the `OcrUnavailable` struct and `lock_ingest_dir` (#100). Chunk 2 added the
engine-input step (#101). The list below is the state this plan started from. Chunk 3 consumes
`prepare`, `RunDir`, `sweep_stale`, and the store functions as they are on main, not the older
names in that list.

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
2. **JPEGs decode through `jpeg-decoder`.** The app decodes JPEG with `zune-jpeg`, through
   `image`, and `zune-jpeg` has no DCT scaling: its `set_max_width` is a limit, not a scale. It
   also holds two full-size buffers while it converts, so a 16 MP page peaked at 144 MB against
   the 112 MB small-page budget. So Chunk 2 adds `jpeg-decoder` 0.3 as a direct dependency and
   decodes every JPEG with it, at 1/1 within the PDF caps and at a reduced `Decoder::scale`
   above them. Display images are already 8-bit sRGB and orientation-applied, because ingest
   re-encodes CMYK, deep, non-sRGB, and rotated JPEGs. So the checks that matter are a large
   baseline JPEG and a large progressive JPEG, with both output dimensions within the caps, plus
   `cargo about` and `cargo audit`. If a check fails, the fallback treats a large JPEG like a
   PNG: a whole decode under the 200 MB rule, which refuses any 8-bit RGB JPEG above about 66 MP.
3. **Error kinds.** `BookError` gains `OcrUnavailable { reason }` and `OcrStorage { path, error }`,
   where `error` is the OS error's text as a `String`, because `BookError` derives `Clone` and
   `Eq` and `std::io::Error` is neither. Both get a `CommandError` mapping in
   `library/command_error.rs` (its variant-count test goes from 13 to 15) and their messages in `src/lib/errors.ts`. The `OcrUnavailable` struct in
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
    the binary. `recognize` writes its TSV and stderr beside the image, so the pre-check copies
    the image into a `RunDir` first and does not write into the bundle. The method returns
    `Result<(), BookError>`, because `OcrError` has no unavailable kind. The pre-check requires
    the known words in the output, so a truncated `eng.traineddata` fails it.
12. **Dead code between chunks.** Chunks 1 to 4 add code whose production caller lands in
    Chunk 5. CI runs clippy with `-D warnings` twice: on the library target (`ci.yml:241-242`)
    and with `--all-targets` (`ci.yml:457-458`). Both compile the library without `cfg(test)`, so
    test uses do not count in either. So each new module is declared with `#[allow(dead_code)]` and a comment naming the chunk
    that consumes it, as `ocr/mod.rs` does today. Chunk 5 removes every one of them, and both
    clippy runs prove nothing is left unused.
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

Merged in #100.

- [x] Add `OcrUnavailable` and `OcrStorage` (decision 3).
- [x] Generalise the lock helper (decision 6).
- [x] Add `ocr/run_dir.rs`: a `RunDir` guard that creates `scriptorium-ocr-run-<pid>-<nanos>` under
  `std::env::temp_dir()`, holds its `.ocr.lock`, and removes the directory on drop.
- [x] Add the temp sweep: top level only, the `scriptorium-ocr-run-` prefix only, no symlinks, older
  than 1 hour, and only when the lock can be taken. `commands::setup` calls it at startup.
- [x] Tests: a stale and a live directory, a directory without the prefix, a symlink with the prefix,
  and a name clash on create.
- [x] Checks: `cargo test`, clippy on the library and all targets, and the existing ingest lock tests
  unchanged.

### Chunk 2: the engine-input step

Merged in #101.

- [x] Change `JP2_MAX_PIXELS` in `ingest/decode.rs` from `pub(super)` to `pub(crate)`, and
  re-export it from `ingest/mod.rs` as `pub(crate) use decode::JP2_MAX_PIXELS as
  MAX_PAGE_PIXELS`, so the input step reuses ingest's 150 MP limit while `decode` stays private.
  The re-export lands here, with its first consumer, because an unused `pub(crate) use` fails
  clippy's `unused_imports`.
- [x] Add `ocr/input.rs`. It reads the size and colour type with `image::ImageReader`, without
  decoding pixels. Bytes per pixel come from the colour type. It refuses a page over 150 MP, and a
  whole decode whose buffer would pass 200 MB. A progressive JPEG is included: its coefficient
  buffer is the full frame, two bytes a sample a component, and scaling does not shrink it. A
  buffer of exactly 200 MB is allowed. A JPEG whose buffers would pass the 264 MB large-page
  budget together is refused as well, and that budget is not raised. The sum is the coefficient
  buffer plus the scaled sample planes, and for colour the interleaved output too, because those
  are live at once. Greyscale reuses its plane. Exactly 264 MB is allowed. Every JPEG goes through `jpeg-decoder` (decision
  2): at 1/1 within the caps, and at the smallest DCT scale at or above the caps beyond them. It
  resizes to the caps in one pass, never upscales, and streams the RGBA rows as a PNG into the
  run directory, so it holds no second full RGBA copy. It takes the stop flag and the deadline
  (decision 7).
- [x] Add the large-page permit: one slot shared by all books. A page is small only when its
  decode buffer, from the header's size and bytes per pixel, is at most 48 MB. Every other page
  needs the permit. So a 12 MP RGBA16 page, which decodes to 96 MB, is a large page even though
  it is within the size caps. The permit guard is held by the page and dropped on every exit,
  including an error or cancel.
- [x] Add the counting allocator as a `#[cfg(test)] #[global_allocator]` in `lib.rs`, so it
  exists only in the unit-test binary, which can reach the private `ocr::input`. It counts
  allocations made by a thread that has armed it. It also counts allocations and frees on a
  `jpeg-decoder` worker thread when exactly one meter is armed: those threads are named
  `worker thread for component N` and they allocate each component's sample plane. The meter
  matches the operating-system name (Linux keeps only `worker thread f`) rather than
  `thread::current`, which panics once a worker's thread-locals are gone. Any other
  thread is ignored, so tests running in parallel do not count. Each budget test arms its own
  thread after building its fixture, and the budget tests share one mutex, so they run one at
  a time.
- [x] Tests on generated images: a 16 MP and a 100 MP JPEG, both baseline and progressive, a
  16 MP PNG, a WebP page, a 60 MP RGBA PNG (240 MB, refused), a 60 MP RGB8 PNG (180 MB, accepted
  under the large-page permit), a 50 MP 16-bit RGBA PNG (400 MB, refused), a 20 MP 16-bit RGBA
  PNG (160 MB, accepted), and a header claiming 200 MP. Each output is within the caps in both
  dimensions, and each peak is within its budget. Refused pages need only a valid header, so
  their fixtures are written header-first with a short body and never hold a full pixel buffer.
  The large accepted fixtures, the 100 MP greyscale JPEGs and the 60 MP RGB8 PNG, are generated
  once per test run into a temp folder, one at a time under the budget tests' mutex. The 100 MP
  JPEGs are greyscale, so building them needs 100 MB, not 300 MB. CI runners have 7 GB, so this
  fits. Each refusal happens before any decode. A baseline 16 MP JPEG fits the 112 MB small
  budget. A progressive RGB page also keeps two bytes of coefficients per sample per component,
  so a 16 MP progressive page (96 MB of coefficients, about 192 MB with its planes and output) takes the large-page
  permit and stays inside the 264 MB budget. A 13.44 MP progressive RGB page over the caps is
  decoded at 1/1, and its measured peak includes the coefficient buffer and stays inside 264 MB.
  A 25 MP progressive RGB header and a 44.7 MP baseline RGB header are refused on that sum
  before any pixels are decoded. A helper reads the chosen scale without decoding
  pixels and asserts 1/1 for 16 MP and 1/2
  (5000x5000) for 100 MP. Both 100 MP peaks fit the 264 MB large budget, and the baseline peak
  stays under a full-size 100 MB plane, so a decode that skipped the scale fails. The two-book
  permit test checks one holder at a time and a combined peak within 600 MB. A cancel during a
  100 MP decode returns within the measured bound.
- [x] Add `jpeg-decoder` 0.3 without default features (its rayon backend decodes buffers in
  parallel and would break the page budget) and the test-only `jpeg-encoder` 0.7 for progressive
  fixtures. Run `pnpm licenses:gen` and commit `THIRD_PARTY_LICENSES.generated.md`. Then `pnpm
  licenses:check` and `cargo audit` must pass.

### Chunk 3: the job core

This chunk does not add commands, the start after ingest, `RunEvent::Exit`, or `try_lock_dir`.
Those are chunks 5 and 4. `OcrJobs` is built in tests. `ocr/mod.rs` declares `job` and `status`
with `#[allow(dead_code)]` and a comment that chunk 5 is the caller (decision 12).

- Add `ocr/job.rs` and `ocr/status.rs`. The registry owns the pool and one mutex per book, keyed
  by `canonical_key` (decisions 4 and 5). The production pool is half of
  `available_parallelism`, at least 1 and at most 4. Tests pass an explicit size, so the two-book
  test does not depend on the machine. Workers take turns, one page per book per turn. Cancelling
  one book does not stop another. The job does not take `ocr/.job.lock`. Chunk 4 adds
  `try_lock_dir` before the rebuild. In this process the per-book mutex is the write lock.
- A run opens the book with `load_book`. That error is returned unchanged, so a checksum mismatch
  or a permission error keeps its own kind. Only an unreadable or invalid `meta.json` is
  `InvalidPackage`. Each page's `imageSha256` comes from that checked meta. The run id is the
  process id and the clock's nanoseconds, the same source as decision 6. The job passes
  `[Lang("eng")]` only.
- Before any page, the job calls `sweep_stale` on the temp directory with `RUN_ORPHAN_AGE`, then
  the pre-check, then creates `ocr/` with `create_dir` when it is missing, as ingest does. The
  pre-check is a method on `OcrEngine` and returns `Result<(), BookError>` (decision 11).
  `FakeEngine` succeeds. This chunk copies `tests/fixtures/ocr/smoke.png` to
  `src-tauri/resources/ocr-check.png` and adds that file to `bundle.resources` in
  `tauri.conf.json`. `TesseractEngine` stores the check-image path, from the resource directory
  or the `CARGO_MANIFEST_DIR` fallback, the same way it finds `tessdata`. It copies that image
  into a `RunDir` and recognises the copy, because `recognize` writes its TSV and stderr beside
  the image and the bundle directory is not writable. A failure is `OcrUnavailable`. No page is
  attempted, and none is marked `failed`.
- The job then calls `rebuild_index`. That function already applies the skip and the forced-run
  exception in `rebuild_entry`, so this chunk does not add a second `is_current` check. It runs
  `pending` pages in order, then retries the pages that were `failed` when the run started, once
  each. A page that fails during this run is not retried until the next run. A recorded failure
  whose `afterRunId` still matches the layer stays `failed` through the rebuild, so the retry
  pass runs it even though the older layer is current. A planted `forced.json` is how this chunk
  proves a current page is redone. Writing, adopting, and deleting that file is chunk 4.
- Each page calls `prepare` in a `RunDir`, with the stop flag and the page deadline (decision 7),
  then `recognize`, then `OcrLayer::new` and `write_ocr_layer` under the per-book mutex. The
  large-page permit stays inside `prepare`. The job does not add another. Dropping the `RunDir`
  removes its directory. The page's input PNG, TSV, and stderr are still deleted when the page
  ends, including on cancel and failure (decision 8). `write_ocr_layer` returns the merged page,
  or the refusal, so the job can read `published_text` from it. An empty `ocr` layer shadows a
  `pdf` layer, so `withText` is recomputed from that page and is not incremented. After a
  successful write the snapshot entry is what `rebuild_entry` would produce for the new layer:
  `Done` only when the layer is current and any forced id is absent or equal to the new run id.
- A cancel from `prepare` or `recognize` leaves the page `pending`, unless this attempt was the
  retry of a page that was `failed` at the start, which stays `failed` with its recorded
  `afterRunId`. A timeout, an engine failure, an input `Failed` (including "too large", and a
  PNG write that fails inside the run directory), or `OcrWrite::Refused` marks that page
  `failed` with a short error, and the job continues. Those are page failures, not
  `unavailable`. Before it records the failure, under the per-book mutex, the job reads the
  layer already on disk and stores that layer's `runId` as `afterRunId`. A `Pending` entry does
  not carry one. `None` is only for a page with no layer. A page or index write that cannot be
  recorded stops the job. The state becomes `unavailable`, and the reason is the error text
  (`OcrStorage` for a disk error). A page that was not written is still `pending` on the next
  rebuild.
- The index is written at most once a second, at once after a page failure, and once more on
  every exit (finished, cancelled, or error) before the registry drops the live snapshot. So
  `book_status` never pairs the last `seq` with an older index.
- `OcrJobs` takes an event sink as well as the engine factory and the resource paths. Chunk 5
  connects that sink to `app.emit`. This chunk's tests record it. There is no `AppHandle` here.
  `commands::session` and `commands::throttle` become `pub(crate)`, so the job can use
  `canonical_key` and `EmitThrottle`. Both are private today. Events follow decision 9. `seq` is
  one counter per book, starts at 0, and is not stored on disk. This chunk emits `running`,
  `cancelling`, `finished`, and `unavailable`. It does not emit `idle` or `busy`: no job reads
  the index, and `busy` is the other instance's lock in chunk 4. Events are built under the
  registry lock and emitted after it is released. Code may hold the per-book mutex and then the
  registry lock, never the reverse.
- Cancel sets the stop flag and the state to `cancelling`. The "within a second" test uses the
  scripted fake, which waits until the stop flag is set. It does not use a real decode. A decode
  already inside `prepare` cannot be interrupted. That wait is the bound chunk 2 measured.
- `book_status(root, jobs)` is what `ocr_status` will call in chunk 5. While a job runs, or while
  `unavailable` is still held, it returns the registry snapshot, including `reason`. Otherwise it
  reads `index.json`. The engine id comes from the factory. If that locate fails, the state is
  `unavailable` with the locate error, and pages are not promoted for an engine mismatch. When
  locate succeeds, a different engine or checksum makes a `done` page `pending`, a missing page
  file makes it `pending`, and a planted `forced.json` keeps a `done` page unfinished unless its
  `runId` is the forced id. A missing or v0 index makes every page `pending`. The state is
  `finished` when `pending` is 0, including a book whose only unfinished pages are `failed`, and
  `idle` when any page is `pending`. `seq` is the last value this process issued for the book, or
  0.
- Extend `FakeEngine` with a script: a result per page index, an optional wait until the stop
  flag is set, and a log of calls. The one-result constructor stays, so the store tests in
  `ocr/tests.rs` keep working.
- Tests on the scripted fake, except where named: resume after cancel; cancel within a second
  with a page in flight; a cancelled retry stays `failed`; a failure is not retried in the same
  run and is retried once in the next; a second request joins the running job; two books share
  the pool and never exceed its size; progress is throttled and the last tick is sent after a
  failure; a progress event sent before the index flush is what `book_status` returns, with that
  `seq`; a page that finishes inside the one-second window is on disk when the snapshot drops; a
  failed page whose layer is still current is retried on the next run; a planted `forced.json`
  makes that current page run again; a "too large" input is a page failure, not `unavailable`;
  on Unix, a read-only `ocr/` folder mid-run stops the job with `OcrStorage` and no later page
  is written. That test is `#[cfg(unix)]`: a read-only directory does not block creates on
  Windows. A reader that queries after an `unavailable` job ended gets that state and its reason.
  With no job, `book_status` covers a different engine, a changed checksum, a missing page file,
  a planted `forced.json`, a missing or v0 index, `finished` when nothing is `pending`, and
  `idle` when something is.
- A fake whose pre-check returns `OcrUnavailable` proves the job attempts no page. The missing
  binary, the non-executable binary, and the missing or truncated `eng.traineddata` are tests in
  `tesseract/tests.rs`. They use a `#[cfg(test)] pub(crate)` constructor and do not call
  `set_var`. `cargo test` runs in one process, and `SCRIPTORIUM_TESSERACT` would race every
  other `locate`.
- The kill path is already tested. `cancel_stops_the_child_within_two_seconds` and
  `a_passed_deadline_returns_timeout` drive the sleeping stand-in through `wait`. This chunk
  does not add another.
- Keep each function at or under the complexity limit of 15. If `job.rs` would pass 600 lines,
  split the pool or the page loop out. Do not raise the limit. Checks: `cargo test --locked
  --lib`, clippy on `--lib --tests` and `--all-targets` with `-D warnings`, and a local Sonar
  scan before every commit and push. The developer changelog notes the job. There is no version
  bump, because the job is not user-facing until chunk 5.

### Chunk 4: forced runs and the cross-instance lock

- `OcrJobs::start(root, force)` writes `ocr/forced.json` atomically before its first page. A
  forced `start` during a run writes a new `runId` at once, and the running job adopts it and
  makes one more pass. Chunk 4 tests call `start`. Chunk 5's `run_ocr` calls it too. The command
  does not exist in this chunk.
- Add `try_lock_dir(dir, name)`, which opens the lock file like `lock_dir` but takes the lock
  with `try_lock`, so it never blocks. The job holds `ocr/.job.lock` with it. An instance that
  cannot take it reports `busy`
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
