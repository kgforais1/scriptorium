# Plan: M2a OCR engine and pipeline

Last reviewed: 2026-11-07
Date: 2026-11-07
Author: planning pass (branch `plan/m2-ocr`)
Status: in-progress
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

**Result (2026-11-09):** Tesseract with `tessdata_fast` is chosen. On the 12 pages it scores 5.06%
CER overall and 1.13% on plain prose, against ocrs's 12.24% and 11.54%. A self-contained build
runs on macOS arm64, Linux x64, and Windows x64. [`docs/ocr.md`](../docs/ocr.md) has the full
results and the build notes. Neither book has a clearly foxed page, so foxing is not covered.

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

The Tesseract engine runs the sidecar as `tesseract <input> <tmpbase> -l <langs> --psm 3 --oem 1
tsv`, which writes `<tmpbase>.tsv`. `<langs>` is `langs` joined with `+`, as Tesseract expects,
for example `eng+fra`. An empty `langs` is a caller bug. `recognize` returns
`OcrError::NoLanguage` before it spawns anything, and never guesses a default. M2a's job always
passes `[eng]`. Output goes to a file, not a pipe, and stderr goes to a capped temp
file, so a full pipe buffer can never stall the child. The engine reads the TSV after the child
exits and parses it. The page row gives the image size. It drops rows with an empty text or a confidence of -1, and
groups words into lines by `block_num`, `par_num`, and `line_num`. It spawns the child, then
polls `try_wait` every 100 ms. It kills the child when `stop` is set or the 120-second deadline
passes, then reaps it. Tesseract starts no child processes of its own, so killing the one process
is enough on every platform. The child runs with `OMP_THREAD_LIMIT=1`, so the job's pool, not
Tesseract, decides how many cores are busy. [ocrs: an in-process call. It checks `stop` and the
deadline between its detection and recognition stages, which is the finest grain the crate
allows. A page that overruns is therefore bounded by one stage, not abandoned to run on.]

**Engine input.** OCR reads the display image, not the original. Boxes therefore match what the reader shows, with
no transform. Only PDF pages are capped at ingest. An image page keeps its full size, and may be
WebP, which a static Tesseract may not read. So the job prepares the engine's input. It reads the
size from the header first, with `image::ImageReader::into_dimensions`, before decoding
anything. (`decode::header_info` does not return a size, and it and `JP2_MAX_PIXELS` are
`pub(super)`, so Phase 4 widens the constant to `pub(crate)` for reuse.) A
page within the PDF caps (4000 px long edge, 12 MP) is decoded as it is. A larger JPEG is decoded
through `jpeg-decoder`'s `Decoder::scale`, which picks the smallest DCT scale (1/8, 1/4, 1/2, or
1/1) whose output is still at or above the requested size. The job requests the capped size, so
the decoded image is never smaller than the caps, and is then resized down to them. The step never
upscales. A 16 MP JPEG therefore decodes at 1/1, because 1/2 would fall below the cap. A 100 MP
JPEG decodes at 1/2. A larger PNG or WebP must be decoded whole. A whole decode is refused when
its buffer would exceed 200 MB. The buffer size comes from the header: pixels times bytes per
pixel, which the PNG header's colour type and bit depth give. So an 8-bit RGBA PNG is refused
above 50 MP, and a 16-bit RGBA PNG, at 8 bytes a pixel, above 25 MP. Any page over 150 MP, the limit ingest applies to JPEG 2000, is refused. A refused page
is marked `failed` with "too large" and never decoded.

**Memory budget.** Every allocation counts: the decode buffer, any conversion to RGB, the resized
buffer, and the PNG encoder's buffers. A page whose decode buffer is within 12 MP is a small page.
At 4 bytes a pixel its decode is at most 48 MB, the resized copy at most 48 MB, and the encoder
at most 16 MB, so 112 MB. Any other page, a scaled JPEG above the cap or a whole PNG or WebP with a
buffer up to 200 MB, is a large page and needs the single large-page permit. Its budget is 200 MB to decode,
48 MB resized, and 16 MB to encode, so 264 MB. There is no separate RGB copy. The resize reads
the decoded buffer in whatever layout it has and writes the capped RGBA output in one pass, so
the only buffers are the three counted. A small page follows the same rule. The step budget is therefore (pool size - 1) x 112 MB +
264 MB, which is 600 MB at the maximum pool of 4. The refusal limits above already fit it. Tests
enforce the budget with a counting allocator. The result is written as a PNG to a temp
file. Engine inputs and outputs live in a per-run directory under
`std::env::temp_dir()`, held by a guard that removes it on drop, so nothing temporary lands in the
book. A crash skips the guard, so each run directory is named `scriptorium-ocr-<random>` and holds
an exclusively locked `.ocr.lock`. `library/sweep.rs` hard-codes `.ingest.lock` today, so Phase
4 generalises its helper to `lock_dir(dir, name)` and uses it for both. At startup, and before each job, the app removes `scriptorium-ocr-*`
directories that are older than 1 hour and whose lock it can take. A live job holds its lock, so
its directory is never removed. A test leaves a stale directory and a live one, and checks only
the stale one goes. The existing sweep only scans a book's top level, so it would not have caught files in
`ocr/` anyway. Boxes are normalised by the size of the image the engine saw,
so scaling does not move them. The input is always the display image path from `meta.json`.
Ingest has already turned TIFF and JPEG 2000 into JPEG or PNG, so the step sees only JPEG, PNG,
and WebP. Tests run a 16 MP display JPEG, a 100 MP JPEG, a 16 MP PNG, a WebP display page, a
60 MP PNG, and a header claiming 200 MP. They check each output is within the PDF caps. A counting
allocator checks each page's peak against its budget. The 16 MP JPEG decodes at 1/1, so its full
48 MB buffer is expected, within the 264 MB large-page budget. The 100 MP JPEG must peak below that
budget, which proves it never decoded at full size (400 MB). The 60 MP PNG, a 50 MP 16-bit RGBA PNG,
and the 200 MP header are refused without decoding. A 20 MP 16-bit PNG is accepted within budget.
A test runs large pages from two books at once, so both wait on the one large-page permit. It
checks only one holds it at a time, and the combined peak stays within 600 MB.

### Storage: `ocr/` schema v1

`ocr/<NNNN>.json` holds one page. It keeps each source's text in its own layer:

```json
{
  "schema": 1,
  "layers": {
    "ocr": {
      "engine": "tesseract 5.5.1 eng-fast",
      "generatedAt": "2026-11-07T12:00:00Z",
      "runId": "…",
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
  "pages": {
    "0": { "status": "done", "imageSha256": "…", "runId": "…" },
    "1": { "status": "failed", "error": "timeout", "failedAt": "…", "afterRunId": "…" }
  },
  "summary": { "pages": 202, "done": 201, "failed": 1, "pending": 0, "skipped": 0, "withText": 180 }
}
```

`status` is one of `pending`, `done`, `failed`, or `skipped`. A plate with no text is `done`
with an empty `text` and no lines. `skipped` is reserved for M2c ("curator says do not OCR").
`pages` lists every page in the book, not only PDF pages. A `done` entry repeats its page's
`imageSha256`, so `ocr_status` can spot a stale page without opening page files.

`summary.pages` is `meta.json`'s page count. `done`, `failed`, `pending`, and `skipped` count the
statuses, and add up to `pages`. `withText` counts pages whose published text is not empty.

**Which pages have a file.** A page file exists only once some layer has content. Ingest writes
one for each PDF page that has text, with a `pdf` layer, as M1.5 does now. The job writes one for
every page it finishes, including a plate with an empty `ocr` layer. So "no file" always means
"nothing recorded yet", and is valid.

**Reading v0.** M1.5 writes v0, which has no `schema` field. A file is read as v0 only if it
validates against `ocr.v0.schema.json`. A file that is neither valid v0 nor valid v1 is invalid,
and the rule under Errors applies. A valid v0 page file reads as a v1 file with only a `pdf`
layer. That layer needs no checksum, because only the `ocr` layer is checked for staleness. The
file is written as v1 the next time the job finishes that page. v0 has never shipped in a
release, so this is a small, tested shim, not a migration. M1.5's ingest switches to writing v1
in Phase 2, and `ocr.v0.schema.json` is kept for the shim's tests.

**Writing.** Today `library/ocr.rs` writes with plain `std::fs::write`, which is not crash-safe.
Phase 2 switches it to the existing `write_atomic` in `library/mod.rs`, so a crash leaves the old
file or the new one. Every `ocr/` write goes through it.

**The index is a cache.** The page files are the truth for `done`. The job writes the index at
most once per second, so after a crash it can lag. A page failure is the exception: it writes
the index at once, outside the throttle. Failures are rare, so this costs little, and a crash can
lose only `done` entries, which step 2 recovers from the page files. Before it starts, the job rebuilds the index
in two steps:

1. It reads the old index if it is valid v1. A missing, v0, or invalid old index counts as empty.
2. It walks `0..meta.pages.len()` from `meta.json`, and applies the first rule that fits.
   - A `skipped` entry in the old index stays `skipped`, even over a matching `ocr` layer. A
     curator's "do not OCR" beats everything, so no run, forced or not, touches that page.
   - A page whose file is invalid is `failed`, with the validation error.
   - A `failed` entry in the old index stays `failed` with its error if the page's `ocr` layer
     still carries the `afterRunId` the failure recorded, or the page still has no `ocr` layer
     and the failure recorded none. `afterRunId` is the `runId` of the layer that was on disk
     when the page failed. So a forced run that fails on a page that had good text stays failed.
     A later success writes a layer with a new `runId`, which no longer matches, and clears it.
     This compares ids, not times, so a clock that moves backward cannot revive a failure.
   - An `ocr` layer with the current engine id and the page's current `sha256` is `done`, unless
     `ocr/forced.json` names a forced run and the layer's `runId` is not that run's id (below). Then
     the page is `pending`. This holds after a crash too, when no job is running.
   - Any other page is `pending`.

Every move to `failed` records `afterRunId`, and stamps `failedAt` for display only: a live page
failure, a timeout, and an invalid file found by the rebuild. No rule compares timestamps.

Every run gets a random `runId`, and every `ocr` layer it writes records it. A forced run is recorded
in its own small file, `ocr/forced.json` (`{ "runId": "…" }`), written atomically. The file exists
only while a forced run is unfinished. A forced run's progress
is therefore an exact match, not a clock comparison, so clock changes cannot mark a page done.

**Why a separate file.** The index is a cache and can be lost. Forced intent cannot be rebuilt from
page files, so it lives apart from the index, and losing the index never loses it. A missing
`forced.json` simply means no forced run, so a first run on an M1 book, which has no index and
no `forced.json`, needs no special case and shows no warning. An invalid `forced.json` is the
one case where intent is unknown. The job then deletes it, treats the run as not forced, and
logs it. It shows no message, because no re-run is known to have happened. Tests cover a lost
index during a forced run, which still resumes the forced pages, a first run with no files, and
an invalid `forced.json`.

Then it writes the rebuilt index atomically. The job runs `pending` pages, then retries each
`failed` page once per run. M2c moves `skipped` into the page file, where it belongs. Reading 202
small JSON files takes milliseconds. Tests cover a crash after a few pages, a missing and an
invalid old index (no page comes back `skipped`), and a carried-over failure.

**Cancel and timeout.** A page in flight when `cancel_ocr` lands goes back to `pending`, because
it never failed. A page that hits the deadline or crashes the engine is `failed`, and that status
is written to the index at once, outside the throttle, under the per-book mutex. Every index
write takes that mutex, so two workers' writes never race.

**`ocr_status`.** It never parses page files. While a job runs, it returns the registry's live
snapshot only, and computes the summary from that snapshot, because the index on disk lags. With
no job running, it reads `index.json`. It checks each `done` entry's `imageSha256` against
`meta.json`, and the index's `engine` against the engine the app ships now. If `ocr/forced.json`
names a run, a `done` entry whose `runId` is not the forced run's id is unfinished. It also checks that each
`done` entry's page file exists, which is a cheap `stat` and no read. Any of these reports that
page as `pending`, so the reader never says "Text ready" over stale text or text
from an older engine. A page file that exists but is invalid is caught by the next job's
rebuild, which marks it `failed`, because only a parse can find it. Pages that are done and
untouched by a forced run stay `done`, so Resume never redoes them. It recomputes the summary
after those checks. A crashed run may still
show a few finished pages as `pending` until the next job starts. That errs the safe way.

**Books with no v1 index.** `ocr_status` treats a missing `index.json`, which every M1 image book
has, as every page `pending`. It treats a valid v0 index the same way. It never fails on either.
The next job writes a v1 index.

**Guarantee.** The job writes only the `ocr` layer of a page file. It reads the file, replaces
`layers.ocr`, and writes it back. It never rewrites a file it cannot parse and validate, so a
damaged file that still holds curator text is left byte-for-byte as it is. Two tests check this.
OCR over a page with `pdf` and `curator` layers leaves both layers identical. OCR over an invalid
file leaves the file unchanged and marks the page `failed`.

### The OCR job

- One job runs per book at a time, across app instances too. The app does not stop a second
  instance from opening, so a job takes an exclusive lock on `ocr/.job.lock`, with the same
  `lock_dir` helper, before it rebuilds the index. Only the lock holder writes the index, the
  `ocr` layers, or `ocr/forced.json`, so two instances cannot overwrite each other's index or erase a failure. The lock
  is released when the job finishes, fails, or is cancelled, and the OS releases it if the app
  crashes. An instance that cannot take the lock reports the state `busy`, writes nothing, and
  its reader shows "Reading text in another window". A forced request there is refused
  with `busy` and writes no `forced.json`. "Recognise text again" is disabled while `busy`, and
  the curator asks again once the other window is done. While `busy`, the reader calls `ocr_status`
  every 5 seconds. `ocr_status` tries the lock without blocking, and releases it at once if it
  gets it. Getting it means the other instance has finished, failed, or crashed, since the OS
  frees the lock of a dead process. The state then leaves `busy` for `idle` or `finished`, read
  from the index, and Resume reappears if pages are unfinished. Tests cover the other instance
  finishing and being killed, and check the reader leaves `busy` within one poll. M2c's editor
  takes the same lock for its writes. Within one instance, a second non-forced request returns
  the running job, and a forced one upgrades it, as described below. A test holds the lock from a
  second process and checks the first writes nothing.
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
- `run_ocr` takes `force: bool`. A forced run re-runs every page that is not `skipped`, including
  fresh `done` pages with the same engine and checksum. It still writes only the `ocr` layer. The
  automatic run after ingest is not forced.
- A forced run is made durable by `ocr/forced.json`, written atomically before its
  first page. While it exists, a page counts as unfinished until its `ocr` layer carries
  that `runId`. So a forced run that is cancelled, or ends in a crash, resumes on its unfinished
  pages only, and existing text stays readable meanwhile. The file is deleted when the forced
  run has been through every page.
- A forced request while a run is going does not just return that run. It writes `ocr/forced.json` with a new
  `runId` atomically at once, under the per-book mutex. The running job adopts that
  id for every page it writes from then on, and then makes one more pass over the pages whose
  layers carry a different id. Because the file is on disk
  before the request returns, a cancel or crash before that pass does not lose it. So
  "Recognise text again" during the automatic run is never lost. A test covers this overlap.
- Before the first page, the job checks the engine once. For Tesseract that means the binary
  starts, and that it recognises a small bundled test image, one line of known text, with every
  requested language and the bundled `--tessdata-dir`. `--list-langs` alone only lists file
  names, so a corrupt or truncated `eng.traineddata` would pass it. Recognition must load the
  data, so it would not.
  If either fails, the whole job fails with `OcrUnavailable` and no page is attempted. A missing
  language therefore never shows up as 202 page failures. A page that fails later for a language
  reason is still a page failure, since the check already passed.
- It reports with two global events, as ingest emits `ingest-progress`. An event, not a
  channel, because a job started after ingest has no command in flight to carry one.
  - `ocr-progress` is `{ root, seq, done, failed, total }`, throttled to 4 a second with the
    existing throttle.
  - `ocr-state` is `{ root, seq, state }`, sent on every change, never throttled. `state` is
    `idle`, `running`, `cancelling`, `finished`, `unavailable`, or `busy`.
  - `seq` is one counter per book that rises with every event.
- `ocr_status(root)` returns the book's job `state`, the current `seq`, and the page statuses, as
  one snapshot. While a job runs, the registry holds the live statuses and counts in memory, and
  every event is built from that state. `ocr_status` reads the state and `seq` together under the
  registry's lock, so its counts are never older than its `seq`, even when the index on disk lags.
  With no job running, it reads `index.json`, and `seq` is the last one the registry issued for
that book in this app session. `seq` is not stored on disk. It starts at 0 each launch, and a
reader only ever compares it with events from the same session. The reader subscribes to both events first, then calls `ocr_status`. It applies the
  query result, then applies only events with a higher `seq`. An event that arrives during the
  call is buffered, and dropped if its `seq` is not higher. So a job that starts, finishes, or is
  cancelled while the reader loads is never missed and never applied twice. A frontend test
  replays events in each order around the query. A Rust test emits progress before the index
  flush, and checks `ocr_status` returns the new counts with that event's `seq`.
- A page failure (timeout, a crash, unreadable output) marks that page `failed` with a short
  error and moves on. If that failure, or any page file or index write, cannot be written, the
  job stops at once, because it can no longer record what it did. It reports `OcrStorage` with
  the path and the OS error, and the state becomes `unavailable`. Nothing is lost: an unwritten
  page has no fresh layer, so the next rebuild finds it `pending` and the next run retries it. A
  test makes the `ocr/` folder read-only mid-run and checks this. The job fails as a whole only if the pre-check fails: the binary will not
  start, or a requested language is missing. That is reported as `OcrUnavailable`, like
  `PdfiumUnavailable`.
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
- `ReaderHeader` gets a "Text" button with three actions. "Recognise text again" calls
  `run_ocr` with `force: true`, so it redoes every page that is not skipped, even unchanged
  ones. "Stop" calls `cancel_ocr`. "Resume" appears whenever no job is running or
  cancelling, that is when the state is `idle`, `finished`, or `unavailable`, and some page is
  `pending` or `failed`. That covers a stopped run, a finished run with failures, a crashed run, a
  book that was never OCRed, and an engine or disk problem that has since been fixed. It calls `run_ocr` with
  `force: false`, so it does only unfinished pages. A frontend test covers each action, that
  Resume stays hidden while `cancelling` or `busy`, and that Resume after `unavailable` calls
  `run_ocr` with `force: false`.
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
- The resource maps to `<resource_dir>/tessdata/eng.traineddata`, and the dev copy sits at
  `src-tauri/tessdata/eng.traineddata`. The child gets `TESSDATA_PREFIX` set to that `tessdata`
  directory and passes `--tessdata-dir` with the same path, so the lookup does not depend on
  Tesseract's search order. The child's environment is cleared first, keeping only what the
  platform needs to start a process, so a system Tesseract or a user's `TESSDATA_PREFIX` can
  never be picked up.
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
| `OcrUnavailable` | The engine binary or language data is missing or will not start. The job's pre-check finds this before any page. |
| `OcrStorage` | A page file or the index cannot be written. The job stops. The next run, once the disk is writable, redoes the unrecorded pages. |
| `OcrError::NoLanguage` | Internal only. An empty `langs` is a caller bug, caught by a unit test, and never reaches the UI. |
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
- `scripts/ocr_spike.py` (Phase 1 only, kept for re-running the comparison on a bump). It
  exits with a notice when the page images are missing. The fixture README lists the exact
  corpus files and the commands that rebuild them.
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

- [x] Add this plan to `to_do.md` (Active plans) and `plans/README.md`. It joins Next Up when it
      is approved and Phase 1 starts.
- [x] Record the M2 split in the roadmap under M2.

### Phase 1: engine spike and decision

- [x] Pick the 12 ground-truth pages and transcribe them. Check each by eye against the scan.
- [x] Write `scripts/ocr_spike.py`. It runs each engine on the 12 pages and prints CER, WER, and
      seconds per page. It must pass ruff and basedpyright.
- [x] Run Tesseract with `tessdata_fast` and with `tessdata_best`, and ocrs.
- [x] Prove the packaging route for the leading engine on macOS arm64: a binary that runs with
      Homebrew moved aside, signed ad hoc under the hardened runtime, inside a built app. Build
      the Linux x64 and Windows x64 binaries in CI, and run each on a runner with no Tesseract
      installed. A failure on any platform fails the gate for Tesseract.
- [x] Apply the gate, write `docs/ocr.md`, and update this plan. Get the choice reviewed before
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

- [ ] Extend `build_tesseract.sh` and `tesseract.yml` (both from Phase 1) to publish the pinned
      binaries. The Windows build's triple is `x86_64-pc-windows-gnu`, but the app targets
      `x86_64-pc-windows-msvc`, so set `TESSERACT_TARGET_TRIPLE` to name it for the app's target.
      Publish the Linux binary from Ubuntu 22.04 (glibc 2.35), as the workflow now builds it.
- [ ] Write `fetch_tesseract.sh`, and run it in the Rust CI jobs as pdfium is fetched.
- [ ] Write `ocr/tesseract.rs`: spawn, timeout, TSV parsing. Unit-test the parser on recorded TSV,
      including empty rows, `-1` confidences, and a multi-block page.
- [ ] Add one integration test that runs the fetched binary on a small committed fixture image,
      through the same path resolution and cleared environment the app uses. It runs in CI on
      macOS, Windows, and Linux. It first asserts that no `tesseract` is on the runner's `PATH`,
      so a pass cannot come from a system install.
- [ ] Add the sidecar and resource config. Run `pnpm tauri build` on macOS and check the built
      app recognises a page.
- [ ] Add a CI job on Windows and Linux that runs `pnpm tauri build`, and then runs the bundled
      sidecar from the bundle's own layout on one fixture page. CI does not build bundles today,
      so this is the first such job. It runs on PRs that touch the OCR scripts or config, and
      weekly. Installing and launching the built app on Windows and Linux stays on the M1
      follow-ups manual list.
- [ ] Add the notices and the bump policy entry.

### Phase 4: job, commands, and UI

- [ ] Write `ocr/job.rs`: pool, ordering, skip rule, progress, cancel, and per-page failure.
- [ ] Add `run_ocr`, `cancel_ocr`, and `ocr_status`, and start a job after ingest.
- [ ] Write the engine-input step: header size, scaled JPEG decode, the large-page permit, the
      refusals, and a temp PNG. Test it on a 16 MP and a 100 MP JPEG, a 16 MP PNG, a WebP page,
      a 60 MP PNG, 50 MP and 20 MP 16-bit PNGs, and a header claiming 200 MP, with the
      counting-allocator check. Add the two-book large-permit test.
- [ ] Tests with `FakeEngine`: resume after cancel, cancel returning within a second while a page
      is in flight, a failing page, a second request joining the running job, two books sharing
      the pool, progress throttling, and cancel on exit.
- [ ] Test that `run_ocr` returns `OcrUnavailable` when `SCRIPTORIUM_TESSERACT` points at a
      missing file, and at a file that is not executable. Test that a missing `eng.traineddata`
      also fails the whole job, with no page attempted and no page marked `failed`. So does a
      truncated `eng.traineddata`, which `--list-langs` would still list, caught by the
      test-image recognition.
- [ ] Test `force`: a forced run redoes a fresh page and keeps a `skipped` one. Test that a
      forced run cancelled halfway resumes on exactly the pages without its `runId`, that a
      lost index keeps the forced run through `forced.json`, and that a failure survives a
      successful-looking clock skew because only `afterRunId` decides. Test that a page
      failure is in the index on disk at once. Test that `ocr_status` reports a `done` page from
      an older engine as `pending`.
- [ ] Test the stale run-directory sweep. Test `recognize` with an empty `langs`.
- [ ] Test the Tesseract engine's kill path with a stub binary that sleeps: both `stop` and the
      deadline end it, and no child is left running.
- [ ] Add the reader status, the Text button, and the error mapping, with frontend tests.
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
- [x] `tessdata_fast` or `tessdata_best`? Decided 2026-11-09: `fast`. The spike found them equally
      accurate, and `fast` is quicker and a quarter of the size.
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
