# Plan: Scriptorium product roadmap

Last reviewed: 2026-10-28
Date: 2026-08-23
Author: bootstrap session
Status: approved
Linked issue/PR: n/a

## Goal

Single source of truth for **all** planned product phases and features. Immediate work lives in
[`archive/completed/2026-08-23-m0-tauri-foundation.md`](archive/completed/2026-08-23-m0-tauri-foundation.md) (M0 complete) →
[`2026-08-23-pre-m1-flipbook-spike.md`](archive/completed/2026-08-23-pre-m1-flipbook-spike.md) (complete) →
[`2026-08-23-m1-scan-viewer.md`](2026-08-23-m1-scan-viewer.md). Later milestones stay here and in
[`to_do.md`](../to_do.md) until promoted. Combined draft:
[`archive/superseded/2026-08-23-mvp-scaffold.md`](archive/superseded/2026-08-23-mvp-scaffold.md).

## Rendering model (long-term)

Three layers, not a single rendering choice:

| Layer | Role |
|---|---|
| **Page images** | Canonical visual record. Originals in any supported input format are retained; the reader shows webview-safe display copies. |
| **OCR + geometry** | Per-page text with word/line bounding boxes (JSON, hOCR, or ALTO). Powers search, select, copy on scans. |
| **Annotations** | Hotspots, lore popups, curator enrichment (W3C Web Annotation–friendly JSON). |

**Reader modes** (book-level `renderMode`, extensible):

| Mode | User sees | When |
|---|---|---|
| `scan` | Flipbook over page images (default MVP) | Authentic book feel |
| `scan+ocr` | Same images + invisible/selectable text layer + search | After M2 |
| `text` | Reflowed or fixed-layout rendered text | M3 — cleaner reading, copy, enrichment |
| `hybrid` | Toggle or split: scan + text side-by-side or overlaid | M4 |

## Look and skins

Reader direction is **mockup E, Lamplit gallery** (decided 2026-10-27). One page-turn engine
and one layout serve every skin; a skin is data only (UI tokens + scene/material config).
Lamplit ships in M1; Gallery Daylight (mockup B) follows in M7. Detail:
[`design/DESIGN.md`](../design/DESIGN.md).

## Milestones

### M0 — Harness & scaffold (complete)

**Completed plan:** [`archive/completed/2026-08-23-m0-tauri-foundation.md`](archive/completed/2026-08-23-m0-tauri-foundation.md)

- [x] CI fast lane + Semgrep + pre-push basedpyright
- [x] Tauri shell + app-data library root + book schema v1 + JSON Schema
- [x] Rust CI: fmt / clippy -D warnings / test / check; commit `Cargo.lock`
- [x] `.envrc.example`, agent tooling contract

### M0.25 — Flipbook spike (complete)

**Completed plan:** [`2026-08-23-pre-m1-flipbook-spike.md`](archive/completed/2026-08-23-pre-m1-flipbook-spike.md)

- [x] ADR: flip library, aspect policy, StrictMode, asset load, skin-ready engine (gates G1–G6) → [ADR-0001](../docs/adr/0001-flipbook-engine.md)
- [x] No silent non-flip fallback

### M0.5 — Harness follow-ups (after Tauri / remaining quality)

Promote when starting. Tracked in [`to_do.md`](../to_do.md).

- [x] Extend CI + pre-push for **TypeScript** lint (tsc already in M0). Done 2026-10-30 in M1 Phase 2 chunk 0 (ESLint, `pnpm typecheck`)
- [x] Frontend Vitest coverage gates (80% lines on `src/lib`, M1 Phase 2 chunk 0). Raising Python `fail_under` stays open below
- [ ] Rust coverage (`cargo llvm-cov` / tarpaulin) when useful
- [ ] Tighten Python harness coverage gate when in-process tests exist
- [x] CodeQL via GitHub default setup (ruleset code scanning; not a workflow file)
- [ ] Dependabot (required before public)
- [ ] GitHub ruleset: required CI + Semgrep checks
- [ ] Note: `cargo fmt` / `clippy` / `test` moved **into M0** (not deferred)
- [ ] **From M0 senior review:** tick archived M0 plan evidence / rephrase roadmap “app-data library root” as decision not delivery (**S9**); refresh stale pyproject/README/annotations wording (**S10**); `tsc -b` + ESLint/Prettier for TS (**S11**); raise pytest `fail_under` (**LC-L3**) — see M1 plan follow-ups table
- [ ] Add `cargo build` to CI Rust job (**LC-M2**) before relying on M1 Tauri commands

### M1 — Scan viewer (MVP core)

**Active plan:** [`2026-08-23-m1-scan-viewer.md`](2026-08-23-m1-scan-viewer.md) (after M0 + flipbook spike ADR)

- [ ] Atomic folder ingest → book package (relative paths, checksums); reject non-slug `id` at ingest
- [ ] Page-turn reader per ADR (not assumed StPageFlip)
- [ ] Persist last-read page; Open… inspect-and-branch
- [ ] Perf budget + error taxonomy tests
- [ ] Close remaining M0 review format/load items (**A10–A13**, **LC-M2** cargo build, **LC-M3** CSP tracking) — see follow-ups section below

**Out of scope for M1:** library shelf, authoring, OCR, hotspots, web deploy.

### M1.5 — PDF import

- [ ] Rasterize PDF pages at ingest (pdfium bundled per platform), capped at a long edge of
      4000 px and 12 MP **per page**, close to the M1 gate corpus's ~11 MP pages. No document-wide
      pixel limit; long PDFs rely on M1's windowed page loading
- [ ] Keep the source PDF once in `sources[]`; each page references it with a `frame`
- [ ] Capture any embedded PDF text layer for M2 search

### M2 — OCR-aware scans (text under the image)

**Target:** app is “aware” of text while still showing scans; search, select, copy.

- [ ] Bundled OCR: ship an OCR engine with the app (lead candidate: Tesseract as a Tauri
      sidecar, with language data for the corpora's languages), so OCR works offline with no
      separate install. Evaluate quality on the test corpora before choosing; keep the engine
      behind one interface so PaddleOCR or a cloud engine can be swapped in later
- [ ] Transcription pipeline: ingest → OCR every page (any format: JPEG, PNG, TIFF, JPEG 2000,
      PDF) → word/line boxes → curator correction → published page text. Re-running OCR never
      overwrites curator corrections
- [ ] Store per-page OCR: plain text + word/line `bbox` JSON (one JSON per page under `ocr/`,
      per the layout M1 reserves in `docs/book-format.md`)
- [ ] Full-text search UI; jump to page/highlight hit
- [ ] Hidden text layer aligned to every scan, whatever its image format (select + copy +
      search), not only PDFs with an embedded text layer. The book is a WebGL canvas, so the layer
      is a DOM overlay placed from the engine's projected page geometry; boxes are stored in
      normalized page coordinates (0–1)
- [ ] Embed the page text in exported files: XMP metadata in exported JPEG and PNG pages, and a
      hidden text layer in exported PDFs, so the text travels with the file. M2 owns this export
      path. TIFF export waits until a TIFF encoder is added
- [ ] Curator correction editor: side-by-side scan and text, edit words and line breaks,
      mark a page reviewed; keep the OCR original and the correction history
- [ ] Background OCR job with progress (re-run when pages change)
- [ ] OCR status per page plus a book-level rollup (`status`, `engine`, `generatedAt`), kept in
      `ocr/index.json`, not `meta.json`, so M2 needs no `formatVersion` bump (decided 2026-10-29)
- [ ] Curator-supplied page text (transcription or translation, typed or imported) stored beside
      OCR with a `source` of `ocr`, `pdf`, or `curator`; curator text wins in search and the text layer

### M3 — Rendered text mode

**Target:** optional clean text reading view for enrichment and accessibility.

- [ ] `renderMode: text` reader (fixed-layout first; reflow later if needed)
- [ ] Preserve links to page numbers / scan view for “show original”
- [ ] Curator-friendly editing surface for lore tied to text spans (prep for M5)
- [ ] Export/copy plain text per page or whole book

### M4 — Hybrid & mode switching

- [ ] In-app toggle: scan | text | hybrid (split or overlay)
- [ ] Remember user preference per book or globally
- [ ] Sync selection between text and scan when geometry allows

### M5 — Library & multi-book

- [ ] Bookshelf (library home): an inventory of every book built on this machine, as a grid or
      list with cover thumbnail, title, page count, and reading progress. It scans the library
      root (`<app-data>/books/`) and reads each `meta.json`; a cached index speeds startup but the
      folders stay the source of truth, so a book copied in by hand still appears. Full
      `load_book` re-hashes every file, so the shelf reads and validates `meta.json` only. A
      book whose `meta.json` is missing or invalid is listed as damaged with its `BookError`
      message, never hidden; checksum drift is caught when the book is opened. The cache key is
      `meta.json`'s size and mtime plus the book directory's mtime (files added or removed)
- [ ] Create / import / remove books; remove asks first and moves the package to the OS trash
- [ ] Export a built book as a single `.scriptorium` file: a zip of the package folder, stored
      uncompressed because pages are already compressed. Spec the bundle in
      `docs/book-format.md` (layout, a manifest with `formatVersion` and the app version, no
      absolute paths) before building it
- [ ] Import a `.scriptorium` file or a package folder: verify it with `load_book` (checksums,
      paths inside the package, schema by version) before it joins the library; an `id` that is
      already taken asks to keep both (new id) or cancel, never overwrite. Reject zip entries
      with absolute paths, `..`, or symlinks, and cap the unpacked size
- [ ] Document export and import for curators in the user docs (how to back up a book, move it
      to another machine, and share it)
- [ ] Optional shelves/collections tags in `meta.json` (a new field, so it needs a `formatVersion` bump)

### M6 — Authoring & hotspots

**Audience:** librarians and gallery directors.

- [ ] Author mode: draw region on page (`xywh` or polygon, in normalized page coordinates)
- [ ] Attach lore popup content (markdown/HTML, images, links)
- [ ] Long-press on touch / click on desktop → popup
- [ ] Expandable annotations: a marker opens a short note, which expands into the full notes
      panel (long text, images, links); keyboard and screen-reader operable per `design/DESIGN.md`
- [ ] Curator detail views: the curator marks a region as a detail; readers click or tap it to
      open a zoomed view of that region at full scan resolution, with a caption. Until M7's
      deep-zoom panel lands, the detail opens as a scaled crop in the notes panel
- [ ] Import/export annotations JSON (interchange with IIIF-style bodies later)

### M7 — Polish & extraction

- [ ] Figure/image extraction from page regions (crop to assets)
- [ ] Deep-zoom inspect on a page or region (OpenSeadragon-style panel), with pinch-to-zoom on touch;
      also renders M6 curator detail views
- [ ] Fullscreen page view: open one page alone, fitted to the screen at full scan resolution,
      with keyboard, swipe, and Escape to return to the book
- [ ] Book polish beyond the M1 engine: page thickness, cover open/close, richer paper detail
- [ ] Second skin **Gallery Daylight** (mockup B) + skin picker; remember choice per user
- [ ] Performance: lazy load pages, virtualized thumbnail scrubber (basic scrubber ships in M1), large book handling

### M8 — Platform & interop (later)

- [ ] PWA or static web build of reader (read-only first)
- [ ] IIIF Presentation export/import (optional GLAM interop)
- [ ] Hosted multi-user CMS / sync (only if product needs it)
- [ ] Optional vector semantic search over OCR (separate from page-image fidelity)
- [ ] OCR engine upgrade path (e.g. PaddleOCR) if Tesseract quality insufficient

## Documentation and help (cross-cutting)

User-facing docs grow with each milestone instead of landing at the end.

- [ ] M1: getting-started guide and a short tutorial in `docs/user/` (see the M1 plan, Phase 4)
- [ ] Each later milestone updates the guide for its feature (PDF import, OCR and corrections,
      library, export and import, authoring, skins) in the same change that ships it
- [ ] Onboarding: a first-run screen that offers the sample book and the tutorial, skippable
      and never shown again once dismissed
- [ ] In-app help (M7): a Help panel that renders the `docs/user/` pages offline, a keyboard
      shortcut sheet, and context links from error dialogs to the page that explains the fix
- [ ] A docs check in CI so user docs keep working links and current screenshots

## Book package schema (evolving)

**Source of truth:** authored in M0 as `docs/book-format.md` + `docs/book-format.schema.json` (split into `v1` and `v2` schemas in M1)
(see [`archive/completed/2026-08-23-m0-tauri-foundation.md`](archive/completed/2026-08-23-m0-tauri-foundation.md)). Do not treat this
roadmap as schema authority.

Those files now exist and win over this section. M1 moves to `formatVersion` 2 (originals);
see `docs/book-format.md`. Historical v1 minimum (aligned with M0):

- `formatVersion` = 1
- `id`, `title`, `createdAt`, `updatedAt`
- `renderMode` (`scan` | reserved `text` / `hybrid`)
- `lastReadPage`
- `rights`, `attribution`
- `pages[]`: `index`, `file` (relative), `width`, `height`, `byteSize`, `sha256`; optional
  `pageLabel`, `storage`
- Reserved: `pages/`, `ocr/`, `annotations.json`

Illustrative shape (field names match M0 — not the superseded `image` / `defaultView` draft):

```json
{
  "formatVersion": 1,
  "id": "uuid",
  "title": "string",
  "renderMode": "scan",
  "lastReadPage": 0,
  "rights": "",
  "attribution": "",
  "pages": [
    {
      "index": 0,
      "file": "pages/001.jpg",
      "width": 2400,
      "height": 3200,
      "byteSize": 1234567,
      "sha256": "hex"
    }
  ],
  "createdAt": "ISO-8601",
  "updatedAt": "ISO-8601"
}
```

## Open questions (roadmap)

- [x] React vs Svelte for Vite UI — React (scaffolded in M0)
- [ ] OCR runtime: bundled in-app engine is the direction (2026-10-29); pick the engine and its packaging in M2
- [ ] Fixed-layout text vs reflow for M3 (likely fixed-layout first for scanned books)

## Related docs

- [`.context/project-profile.md`](../.context/project-profile.md) — stack and constraints
- [`inventory/virtual-books-flipbook.md`](../inventory/virtual-books-flipbook.md) — library menu
- [`to_do.md`](../to_do.md) — prioritized backlog index
