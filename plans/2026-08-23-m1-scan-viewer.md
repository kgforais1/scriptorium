# Plan: M1 — Scan viewer (first product milestone)

Last reviewed: 2026-10-28
Date: 2026-08-23
Author: bootstrap session (revised after Hy3 + ox-alpha plan reviews)
Status: draft
Linked issue/PR: n/a
Depends on:
- [`archive/completed/2026-08-23-m0-tauri-foundation.md`](archive/completed/2026-08-23-m0-tauri-foundation.md) **complete**
- [`2026-08-23-pre-m1-flipbook-spike.md`](2026-08-23-pre-m1-flipbook-spike.md) **complete** (ADR gates G1–G6)
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

**Image-first reader** using the **library chosen in the pre-M1 ADR** (not assumed StPageFlip).
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
| A1 | Ingest of a fixture folder produces a package that **validates** against `docs/book-format.schema.json` | `cargo test` + schema validation |
| A2 | Page order matches the **natural-sort table** below for the committed sort fixture | `cargo test` |
| A3 | Each defined error variant returns its required user-visible message and does not abort the process | `cargo test` (+ manual spot-check in UI) |
| A4 | Reader shows **page-turn** for ≥2 pages using ADR library; images are scan files (not placeholders); **no aspect stretch** (pad policy from ADR) | Manual + spike gates carried forward |
| A5 | Keys: **←/→** and on-screen prev/next; at last page next is no-op; at first page prev is no-op | Manual script |
| A6 | `lastReadPage` persists across quit/relaunch for the same book **`id`** (not filesystem path); progress flush on window close (not only debounced flip) | Manual script |
| A7 | Open… on an existing package directory loads without re-copy | Manual script |
| A8 | Unsupported `formatVersion` → `SchemaVersionUnsupported` message | `cargo test` |
| A9 | Perf budget (below) holds on the **named reference class** against the **pinned gate corpus** | Manual timed run; record results in PR |
| A10 | Mutating one page byte causes `ChecksumMismatch` on load (negative checksum test) | `cargo test` |
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
| `ChecksumMismatch` | Page bytes ≠ stored sha256 | Book files corrupted or modified |
| `PermissionDenied` | OS denied | Permission denied |

### Input formats (normative for M1)

Ingest keeps each original file and gives the reader a file every target webview can show.

| Input | Extensions | Reader gets |
|---|---|---|
| JPEG, PNG, WebP | `.jpg` `.jpeg` `.png` `.webp` | The original |
| BMP, GIF | `.bmp` `.gif` | A PNG copy (GIF: first frame only) |
| TIFF | `.tif` `.tiff` | A JPEG copy at quality ≥ 92; each frame of a multi-page TIFF is one page |
| JPEG 2000 | `.jp2` `.j2k` `.jpf` | A JPEG copy at quality ≥ 92 (OpenJPEG binding) |

Ingest rules for every format:

- Apply the EXIF orientation before measuring `width` and `height`.
- Convert CMYK, 16-bit, and ICC-tagged images to 8-bit sRGB in the display copy.
- Keep the original, unmodified, beside the display copy. Checksums cover both files.
- A decode failure is `UnreadableFile` for that page, not a silent skip.

Storing originals adds a field to page entries, so it needs `formatVersion` 2. See
[`docs/book-format.md`](../docs/book-format.md#planned-originals-and-display-copies).

### Natural-sort specification (normative for M1)

Sort **filename stem** with natural order (numeric chunks as integers), case-insensitive
extension ignored for ordering. Only the supported input formats below are included.

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

Expand to ≥12 cases in `cargo test` beside the sort function. Do not leave tie-breaks as open questions.

### Performance budget (M1)

**Reference machine class (named up front):** Apple Silicon Mac (M-series), **16 GB RAM**,
development build via `pnpm tauri dev` (or release build if noted). If developing primarily
on another class, amend this plan before claiming A9.

**Gate corpus (pinned):** *Edmund Dulac's Picture-Book*, archive.org
`edmunddulacspict00dulauoft`: **204 pages** at about **11 MP** each (2808 × 3906), JPEG
quality 92, about 450 MB total. Fetch it with the recipe in
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

- [ ] Confirm ADR library still loads in current shell
- [ ] Complete asset-protocol checklist above (macOS required; Windows if available)
- [ ] Stop if G1–G6 from spike no longer hold

### Phase 1 — Book I/O (Rust)

- [ ] Natural sort + table tests
- [ ] Atomic create_book_from_folder + free-space check + DestinationExists
- [ ] Per-page width/height/sha256/byteSize written
- [ ] load_book + schema validation
- [ ] lastReadPage read/write keyed by book **`id`**; flush API for window close
- [ ] load_book re-verifies per-page sha256 (negative test: A10)
- [ ] load_book canonical path prefix check (A12)
- [ ] Ingest case-insensitive `id` collision check (A11)
- [ ] Orphan temp-dir sweep + DuplicatePageStem tests
- [ ] All error taxonomy variants tested with `tempfile`
- [ ] Schema bind: Rust sample serialize ↔ `docs/book-format.schema.json`

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
- [`2026-08-23-pre-m1-flipbook-spike.md`](2026-08-23-pre-m1-flipbook-spike.md)
- [`2026-08-23-product-roadmap.md`](2026-08-23-product-roadmap.md)
