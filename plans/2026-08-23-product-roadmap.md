# Plan: Scriptorium product roadmap

Last reviewed: 2026-10-27
Date: 2026-08-23
Author: bootstrap session
Status: approved
Linked issue/PR: n/a

## Goal

Single source of truth for **all** planned product phases and features. Immediate work lives in
[`archive/completed/2026-08-23-m0-tauri-foundation.md`](archive/completed/2026-08-23-m0-tauri-foundation.md) (M0 complete) →
[`2026-08-23-pre-m1-flipbook-spike.md`](2026-08-23-pre-m1-flipbook-spike.md) →
[`2026-08-23-m1-scan-viewer.md`](2026-08-23-m1-scan-viewer.md). Later milestones stay here and in
[`to_do.md`](../to_do.md) until promoted. Combined draft:
[`archive/superseded/2026-08-23-mvp-scaffold.md`](archive/superseded/2026-08-23-mvp-scaffold.md).

## Rendering model (long-term)

Three layers, not a single rendering choice:

| Layer | Role |
|---|---|
| **Page images** | Canonical visual record (scanned JPEG/PNG/WebP). Always retained. |
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

### M0.25 — Flipbook spike (blocks M1)

**Active plan:** [`2026-08-23-pre-m1-flipbook-spike.md`](2026-08-23-pre-m1-flipbook-spike.md)

- [ ] ADR: flip library, aspect policy, StrictMode, asset load, skin-ready engine (gates G1–G6)
- [ ] No silent non-flip fallback

### M0.5 — Harness follow-ups (after Tauri / remaining quality)

Promote when starting. Tracked in [`to_do.md`](../to_do.md).

- [ ] Extend CI + pre-push for **TypeScript** lint (tsc already in M0)
- [ ] Frontend Vitest coverage gates; raise `fail_under` for app code
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

### M2 — OCR-aware scans (text under the image)

**Target:** app is “aware” of text while still showing scans; search, select, copy.

- [ ] OCR pipeline (evaluate: Tesseract via Rust sidecar vs batch CLI vs tesseract.js)
- [ ] Store per-page OCR: plain text + word/line `bbox` JSON
- [ ] Full-text search UI; jump to page/highlight hit
- [ ] Transparent text layer aligned to scan (select + copy)
- [ ] Background OCR job with progress (re-run when pages change)
- [ ] Book metadata: `ocrStatus`, `ocrEngine`, `ocrGeneratedAt`

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

- [ ] Library home: grid/list of books (cover thumbnail, title, progress)
- [ ] Create / import / remove books
- [ ] Standalone book bundles (portable folder or `.scriptorium` archive)
- [ ] Optional shelves/collections tags in `meta.json`

### M6 — Authoring & hotspots

**Audience:** librarians and gallery directors.

- [ ] Author mode: draw region on page (`xywh` or polygon)
- [ ] Attach lore popup content (markdown/HTML, images, links)
- [ ] Long-press on touch / click on desktop → popup
- [ ] Import/export annotations JSON (interchange with IIIF-style bodies later)

### M7 — Polish & extraction

- [ ] Figure/image extraction from page regions (crop to assets)
- [ ] Deep-zoom inspect on a page or region (OpenSeadragon-style panel)
- [ ] Book polish beyond the M1 engine: page thickness, cover open/close, richer paper detail
- [ ] Second skin **Gallery Daylight** (mockup B) + skin picker; remember choice per user
- [ ] Performance: lazy load pages, virtualized thumbnail scrubber (basic scrubber ships in M1), large book handling

### M8 — Platform & interop (later)

- [ ] PWA or static web build of reader (read-only first)
- [ ] IIIF Presentation export/import (optional GLAM interop)
- [ ] Hosted multi-user CMS / sync (only if product needs it)
- [ ] Optional vector semantic search over OCR (separate from page-image fidelity)
- [ ] OCR engine upgrade path (e.g. PaddleOCR) if Tesseract quality insufficient

## Book package schema (evolving)

**Source of truth:** authored in M0 as `docs/book-format.md` + `docs/book-format.schema.json`
(see [`archive/completed/2026-08-23-m0-tauri-foundation.md`](archive/completed/2026-08-23-m0-tauri-foundation.md)). Do not treat this
roadmap as schema authority.

Until those files exist, schema **v1** minimum (aligned with M0):

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
- [ ] OCR runtime: in-app Rust/Tesseract vs external batch vs WASM
- [ ] Fixed-layout text vs reflow for M3 (likely fixed-layout first for scanned books)

## Related docs

- [`.context/project-profile.md`](../.context/project-profile.md) — stack and constraints
- [`inventory/virtual-books-flipbook.md`](../inventory/virtual-books-flipbook.md) — library menu
- [`to_do.md`](../to_do.md) — prioritized backlog index
