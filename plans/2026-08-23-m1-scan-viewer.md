# Plan: M1 — Scan viewer (first product milestone)

Last reviewed: 2026-10-28
Date: 2026-08-23
Author: bootstrap session (revised after Hy3 + ox-alpha plan reviews)
Status: in-progress (approved 2026-10-29)
Linked issue/PR: n/a
Depends on:
- [`archive/completed/2026-08-23-m0-tauri-foundation.md`](archive/completed/2026-08-23-m0-tauri-foundation.md) **complete**
- [`archive/completed/2026-08-23-pre-m1-flipbook-spike.md`](archive/completed/2026-08-23-pre-m1-flipbook-spike.md) **complete** (ADR gates G1–G6 pass): [ADR-0001](../docs/adr/0001-flipbook-engine.md)
Follows: product roadmap M1

## Goal

Deliver the **first usable product slice**: open a folder of scanned page images → create a
**book package** → **page-turn** through faithful page images (no stretch) with persisted
last-read page. Priorities: reliability and correctness over polish.

## Out of scope

- OCR / search / text layer (M2)
- Text / hybrid reader modes (M3–M4)
- Multi-book shelf UI (M5) — reopen package is enough
- Hotspot authoring (M6)
- Deep-zoom, figure extraction, heavy texture (M7) — except whatever the flip ADR already chose
- Web/PWA / IIIF (M8)
- Referenced/symlink storage mode (schema-reserved only)
- PDF import (M1.5)
- HEIC and AVIF (document unsupported; revisit later)

## Approach

**Image-first reader** using the engine chosen in [ADR-0001](../docs/adr/0001-flipbook-engine.md): our own WebGL2 port of mockup E, pad-to-uniform aspect. Start from the spike in `src/spike/flipbook/` and follow the ADR's rewrite scope.
Ingest per M0 decisions: natural-sort → **atomic copy** into app-data `books/<id>/` with
**relative paths only** in `meta.json`. Resolve to absolute paths only in Rust at load time for
`convertFileSrc`.

**UX:** single **Open…** action that inspects the chosen folder — if `meta.json` present, load
book; else treat as scan folder and ingest (confirm dialog before copy). Avoid two near-identical
folder pickers.

**No silent non-flip fallback.** If the ADR library fails in integration, stop and escalate.

### Alternatives considered

| Option | Why not |
|---|---|
| PDF-only ingest | Product is folder-of-scans |
| Dual Open folder / Open book pickers | Confusing; use inspect-and-branch |
| `<img>` strip if flip fails | Deletes the product metaphor |
| Defer lazy-load always to M7 | Only if perf budget met; else windowed load is **in** M1 |

## Acceptance criteria (binary)

| ID | Criterion | How verified |
|---|---|---|
| A1 | Ingest of a fixture folder produces a package that **validates** against `docs/book-format.v2.schema.json`; v1 fixtures still validate against `docs/book-format.v1.schema.json` | `cargo test` + schema validation |
| A2 | Page order matches the **natural-sort table** below for the committed sort fixture | `cargo test` |
| A3 | Each defined error variant returns its required user-visible message and does not abort the process | `cargo test` (+ manual spot-check in UI) |
| A4 | Reader shows **page-turn** for ≥2 pages using ADR library; images are scan files (not placeholders); **no aspect stretch** (pad policy from ADR) | Manual + spike gates carried forward |
| A5 | Keys: **←/→** and on-screen prev/next; at last page next is no-op; at first page prev is no-op | Manual script |
| A6 | `lastReadPage` persists across quit/relaunch for the same book **`id`** (not filesystem path); progress flush on window close (not only debounced flip) | Manual script |
| A7 | Open… on an existing package directory loads without re-copy | Manual script |
| A8 | Unsupported `formatVersion` → `SchemaVersionUnsupported` message | `cargo test` |
| A9 | Perf budget (below) holds on the **named reference class** against the **pinned gate corpus** | Manual timed run; record results in PR |
| A10 | Mutating one byte of a page file, or of a `sources[]` original, causes `ChecksumMismatch` on load | `cargo test` |
| A11 | Ingest rejects `id` that case-folds equal to an existing book directory on case-insensitive FS | `cargo test` |
| A12 | Resolved page path canonicalizes under package root (prefix assertion after `join`) | `cargo test` |
| A13 | Webview CSP is **not** `null` before Phase 2 commands + asset protocol ship | Review `tauri.conf.json` + manual |

### Error taxonomy (Rust enum → user message)

| Variant | When | Message must convey |
|---|---|---|
| `EmptyFolder` | No files | Folder has no files |
| `NoSupportedImages` | No file in a supported input format (see below) | No supported images found |
| `UnreadableFile` | I/O error on a page | Which file failed |
| `DestinationExists` | Target book path exists (incl. case-insensitive `id` collision on macOS/Windows) | Refuse overwrite |
| `DuplicatePageStem` | Two sources map to same stem (incl. case-insensitive) | Duplicate page names |
| `InsufficientDiskSpace` | Pre-flight fail | Need more free space |
| `SchemaVersionUnsupported` | Unknown/newer formatVersion | Unsupported book version |
| `InvalidPackage` | missing meta / schema fail | Not a valid Scriptorium book |
| `ChecksumMismatch` | Page or original bytes ≠ stored sha256 | Book files corrupted or modified |
| `PermissionDenied` | OS denied | Permission denied |

### Input formats (normative for M1)

Every page's `file` is a **display image** that every target webview can show: 8-bit
sRGB JPEG, PNG, or WebP, stored upright. When the input already meets that, it is copied
as-is and there is no separate original. Otherwise ingest writes a converted display copy
and keeps the untouched original under `originals/`.

| Input | Extensions | Display image |
|---|---|---|
| JPEG, PNG, WebP | `.jpg` `.jpeg` `.png` `.webp` | The input itself, unless a conversion rule below applies |
| BMP, GIF | `.bmp` `.gif` | A PNG copy (GIF: first frame only) |
| TIFF | `.tif` `.tiff` | A JPEG copy at quality ≥ 92; each directory (frame) of a multi-page TIFF is one page |
| JPEG 2000 | `.jp2` `.j2k` `.jpf` | A JPEG copy at quality ≥ 92 |

A JPEG, PNG, or WebP input gets a converted copy when any of these is true:

- It is CMYK, 16-bit, or carries a non-sRGB ICC profile. Convert to 8-bit sRGB. An RGB or
  grey image with no profile is treated as sRGB. A CMYK image with no profile is rejected as
  `UnreadableFile`, because the common default CMYK profiles cannot be redistributed.
- Its EXIF orientation is not 1. Rotate or flip the pixels and drop the tag. TIFF uses its
  own orientation tag, with the same rule.

Other rules:

- `width` and `height` describe the stored display image after orientation.
- A decode failure is `UnreadableFile` for that page, not a silent skip.
- Platform floor: macOS 11 or later, because WKWebView decodes WebP only from Safari 14.
  Set `bundle.macOS.minimumSystemVersion` to `11.0`.

Decoder choices, confirmed in Phase 1 with a small spike per crate:

| Need | Candidate |
|---|---|
| JPEG, PNG, WebP, BMP, GIF decode and JPEG/PNG encode | `image` |
| TIFF directory enumeration and orientation | `tiff` |
| ICC and CMYK to sRGB | `lcms2`, statically linked |
| JPEG 2000 | `jpeg2k` over OpenJPEG, statically linked so no runtime library ships |

Originals need a book-level `sources[]` list and a per-page `source` reference, so M1 writes
`formatVersion` 2. See
[`docs/book-format.md`](../docs/book-format.md#planned-originals-and-display-copies).

### Natural-sort specification (normative for M1)

Sort **filename stem** with natural order (numeric chunks as integers), case-insensitive
extension ignored for ordering. Only the formats in [Input formats](#input-formats-normative-for-m1) are included.

**Multi-frame inputs (normative):** sort input files first, then expand each multi-page TIFF
in its directory order, frame 0 first. Every page gets a display file named by its final
0-based page position (`pages/0000.jpg`, `pages/0001.png`, …), never by its input stem, so a
TIFF's frames cannot collide with an ordinary input file. Test a folder that mixes `2.tif`
(three frames) with `1.jpg` and `3.jpg`: the result is `1.jpg`, the three frames, then `3.jpg`.

**Chunk order (normative):** split each stem into alternating numeric and text chunks and
compare them pairwise, left to right. The first chunk pair that differs decides the order:
- Two numeric chunks compare by integer value (`2` before `10`).
- A numeric chunk sorts before a text chunk at the same position, so pure-numeric stems come
  before `cover`.
- Two text chunks compare case-insensitively by UTF-8 bytes.
- A stem that runs out of chunks first sorts first (`1` before `1a`).

So `0a` sorts before `1`: the first chunks are `0` and `1`, and 0 < 1.

**Tie-break (normative):** after natural compare of stems is equal, order by:
1. Longer zero-padded numeric representation wins for that chunk only when integer values
   are equal but digit strings differ (`001` before `01` before `1` — longer pad first).
2. Else lexicographic compare of the full stem as UTF-8 bytes (stable, locale-independent).
3. Else original path string as UTF-8 bytes.

| Input filenames (unordered) | Expected order |
|---|---|
| `2.jpg`, `10.jpg`, `1.jpg` | `1`, `2`, `10` |
| `page-2.PNG`, `page-10.png`, `page-1.png` | `page-1`, `page-2`, `page-10` |
| `img01.jpg`, `img001.jpg` | `img001`, `img01` (longer pad first per rule 1) |
| `cover.jpg`, `1.jpg`, `2.jpg` | `1`, `2`, `cover` (letters after pure-numeric stems that sort as numbers first — pure numeric stems ordered by value; non-numeric follow by UTF-8) |
| `1.jpg`, `1b.jpg`, `1a.jpg` | `1`, `1a`, `1b` |
| `1.jpg`, `0a.jpg` | `0a`, `1` (first chunks compare as 0 < 1) |

Expand to ≥12 cases in `cargo test` beside the sort function. Do not leave tie-breaks as open questions.

### Performance budget (M1)

**Reference machine class (named up front):** Apple Silicon Mac (M-series), **16 GB RAM**,
development build via `pnpm tauri dev` (or release build if noted). If developing primarily
on another class, amend this plan before claiming A9.

**Gate corpus (pinned):** *Edmund Dulac's Picture-Book*, archive.org
`edmunddulacspict00dulauoft`: **202 pages**, JPEG quality 92, about 450 MB total. 196 pages
are 2808 × 3906 (about 11 MP). The covers and a few plates differ (2924 × 4218, 3048 × 4198,
and four at 936 × 1292), which also exercises mixed page sizes. Fetch it with the recipe in
[`tests/fixtures/README.md`](../tests/fixtures/README.md#public-domain-test-corpora).
Pixel count drives decode time and memory (about 44 MB per decoded RGBA page), so the gate
is set by pixels, not file size. Do not upscale pages to inflate file size.

| Metric | Target | Measurement |
|---|---|---|
| Open to first interactive flip | ≤ 15 s | Wall clock from Open confirm → first successful flip gesture |
| Steady-state RSS after open | ≤ 2 GB | Sample process RSS 30 s after first flip (Activity Monitor / `ps`) |
| Flip animation | ≥ 30 fps average over 10 flips | `requestAnimationFrame` deltas while flipping; log mean fps |

If targets fail without lazy/windowed loading, **implement windowed page load in M1**. Soft-warn at **500** pages; no hard cap unless ADR says so.

## Tauri asset-protocol checklist (required before reader Phase)

- [ ] `app.security.assetProtocol.enable: true`
- [ ] **Replace `security.csp: null`** with an explicit CSP before Phase 2 (see A13)
- [ ] `assetProtocol.scope.allow` covers app-data `books/**` (app-wide scope ≠ fs capability)
- [ ] CSP allows `img-src` (and media if needed) for `asset:` and `https://asset.localhost`
- [ ] `convertFileSrc` only given **absolute** paths (Rust resolves relative → absolute)
- [ ] After `package_root.join(file)`, canonicalize and assert result is under `package_root` (A12)
- [ ] macOS: account for `/var` → `/private/var` canonicalization in scopes
- [ ] Windows: verify `https://asset.localhost` form
- [ ] Persisted-scope plugin **not** required for app-data default; revisit if library root changes

## Proposed file changes

```
src-tauri/src/book/     — sort, create (atomic copy), load, validate, progress, errors
src/features/reader/    — FlipbookView per ADR
src/features/home/      — Open… inspect-and-branch
src/lib/bookTypes.ts
docs/manual-test-m1.md  — step script + fixtures
scripts/generate-fixture-book.*  — small + large corpora → tmp/
```

## Phases & checklist

### Phase 0 — Integration gate (ADR already done)

- [x] Confirm ADR library still loads in current shell (spike on `main`, 2026-10-29)
- [ ] Complete asset-protocol checklist above (macOS required; Windows if available)
- [ ] Stop if G1–G6 from spike no longer hold
- [ ] Close ADR-0001's open items: WebGL2 on WebView2 (Windows) and WebKitGTK (Linux), touch on
      real hardware, frame rate in WKWebView, and a process RSS sample

### Phase 1 — Book I/O (Rust)

- [ ] Book format v2: rename the current schema to `book-format.v1.schema.json` and add
      `book-format.v2.schema.json` with `sources[]` and page `source`. The loader reads
      `formatVersion` first, then picks the schema; any other version is
      `SchemaVersionUnsupported` (A8). Rust reads v1 and v2 and writes v2. Add the `sources[]`
      validation rules and the v1→v2 migration note to `docs/book-format.md`. Update every
      reference to the old schema path (`src-tauri/src/book_format.rs` `load_schema`,
      `tests/test_generate_fixture_book.py`, README, `docs/NAVIGATION.md`, `CHANGELOG.md`,
      comments) and give each schema file its own `$id`
- [ ] Format conversion per [Input formats](#input-formats-normative-for-m1), with one
      fixture per row and per conversion rule (CMYK, 16-bit, ICC, EXIF orientation,
      multi-page TIFF)
- [ ] Natural sort + table tests
- [ ] Atomic create_book_from_folder + free-space check + DestinationExists
- [ ] Per-page width/height/sha256/byteSize written
- [ ] load_book + schema validation
- [ ] lastReadPage read/write keyed by book **`id`**; flush API for window close
- [ ] load_book re-verifies per-page sha256 and each `sources[]` sha256 once (negative test: A10)
- [ ] load_book canonical path prefix check (A12)
- [ ] Ingest case-insensitive `id` collision check (A11)
- [ ] Orphan temp-dir sweep + DuplicatePageStem tests
- [ ] All error taxonomy variants tested with `tempfile`
- [ ] Schema bind: Rust sample serialize ↔ `docs/book-format.v2.schema.json` (and v1 samples ↔ v1 schema)

### Phase 2 — Commands + UI

- [ ] Replace `security.csp: null` with production CSP (**A13** — do this **first** in Phase 2 before commands/asset URLs; open tracking issue when starting Phase 2)
- [ ] Commands + capabilities for dialog + app-data fs + asset scope
- [ ] Open… inspect-and-branch + confirm before ingest copy
- [ ] Map errors to dialogs/toasts
- [ ] Vitest (or equivalent) for pure TS helpers (URL resolve / UI state) — required, not optional

### Phase 3 — Reader

- [ ] ADR flip library bound to page list
- [ ] ←/→ + on-screen controls; edge no-ops
- [ ] Debounced progress + **flush on close**
- [ ] Loading / empty states
- [ ] Reader chrome follows mockup E layout (header, bottom bar with scrubber); notes panel
      shell only — hotspots stay M6
- [ ] Skin plumbing per [`design/DESIGN.md`](../design/DESIGN.md): build step turns
      `design/skins/lamplit/tokens.json` into CSS custom properties; engine reads `scene.json`
- [ ] No hard-coded colours, fonts, or scene values in reader components (lamplit is the only
      skin; no picker yet)
- [ ] Every control works by keyboard, has a visible focus state, and has an accessible name
- [ ] Touch: one-finger drag and tap zones turn pages; controls have touch targets of at least
      44 px; hover-only affordances (tooltips, thumbnail lift) have a tap equivalent; the
      canvas blocks browser pinch and scroll without trapping the chrome
- [ ] With `prefers-reduced-motion`, pulses stop and page turns shorten or cross-fade

### Phase 4 — Docs & release

- [ ] `docs/manual-test-m1.md` + README try-it
- [ ] `VERSION` → **0.2.0**; `CHANGELOG.md` user-facing
- [ ] Update roadmap / `to_do.md`; archive this plan when verified

## Verification

- [ ] All acceptance IDs A1–A9 checked
- [ ] Keyboard, accessible-name, and reduced-motion checks above pass in the manual script
- [ ] `cargo test` / `clippy` / `fmt` green in CI
- [ ] Manual script executed once on reference machine; results noted in PR or README
- [ ] No absolute user home paths in committed fixtures or sample `meta.json`

## Open questions

- [ ] Single vs spread layout: follow ADR / library defaults

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| ADR library still painful in product UI | med | high | Phase 0 gate; escalate — no strip fallback |
| Large books OOM | med | high | Budget A9; windowed load if needed |
| Schema drift TS/Rust | med | med | JSON Schema + Rust serialize-and-validate test (M0/M1) |

## Follow-ups from M0 senior reviews (2026-08-25)

Before-M1 items (**S1**, **S2**, **S3**, **S5**) landed on the M0 branch. Critical re-review
(**#3**) added **A10–A13** (checksum negative test, case-fold `id`, path prefix, CSP). M0 branch
also landed **S6** (`deny_unknown_fields`), expanded negative/index tests, slug lockstep tests,
and **D2** annotations wording.

Remaining findings to address during M1 (ingest/load) or harness work:

| ID | Item | Home |
|---|---|---|
| S4 | ~~RFC 3339 timestamps in `BookMeta::validate`~~ — done on M0 branch | — |
| S7 | ~~Unique tempdir + soft-skip without Python~~ — done on M0 branch | — |
| S8 | ~~Table-driven negative tests~~ — largely done on M0 branch; extend if new variants appear | M1 |
| S12 | ~~Clear stale pages on fixture regenerate~~ — done (full `pages/` wipe); drop redundant `cargo check` **or add `cargo build`** still open | next CI touch |
| LC-M2 | Add `cargo build` (debug) to CI Rust job so Tauri/config/icon breakage is caught (LongCat M2). Note: `cargo check`/`clippy`/`test` already succeed without `../dist` (Greptile “missing frontend assets” is a false positive for those steps); re-verify when adding `cargo build`. | next CI / before M1 coding |
| LC-M3 | **A13** first in Phase 2: replace `csp: null`; open a tracking issue when Phase 2 starts (LongCat M3) | M1 Phase 2 / A13 |
| C3 | `load_book` must verify page bytes vs `sha256` — pinned as **A10** | M1 Phase 1 |
| SEC2 | Case-insensitive `id` directory collision — pinned as **A11** | M1 Phase 1 |
| SEC3 | Canonical path must stay under package root — pinned as **A12** | M1 Phase 1 |
| SEC4 | `csp: null` must be replaced — pinned as **A13** | M1 Phase 2 |

Harness/docs (not M1 product):

| ID | Item | Home |
|---|---|---|
| S9 | Archived M0 plan checklist still unticked; roadmap “app-data library root” is a decision, not delivered code | M0.5 / docs hygiene |
| S10 | README hedge; centralize or test slug regex triplication (Rust/Python/schema lockstep tests now exist) | M0.5 |
| S11 | `tsc -b` for project references; ESLint/Prettier for TS | [roadmap M0.5](2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri-scaffold--when-app-code-exists) |
| LC-L3 | Raise pytest `fail_under` from 0 (LongCat L3) | M0.5 |
| LC-T11 | Consider `pageLabel` `maxLength` (schema + Rust) to match title/rights/attribution (LongCat post-Qodo T11) | M1 format hardening |

Sources: `tmp/2026-08-25-m0-tauri-foundation-senior-review.md`,
`tmp/2026-08-25-m0-tauri-foundation-critical-senior-review.md`,
`tmp/2026-08-26-m0-longcat-critical-review.md` (gitignored).

## Completion steps

1. Status → `complete`; move to `plans/archive/completed/`
2. `VERSION` **0.2.0** + changelog
3. Roadmap M1 `[x]`; prune `to_do.md`

## Related

- [`archive/completed/2026-08-23-m0-tauri-foundation.md`](archive/completed/2026-08-23-m0-tauri-foundation.md)
- [`2026-08-23-pre-m1-flipbook-spike.md`](archive/completed/2026-08-23-pre-m1-flipbook-spike.md)
- [`2026-08-23-product-roadmap.md`](2026-08-23-product-roadmap.md)
