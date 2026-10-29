# Virtual Books / Flipbooks / Annotated Scans

Last reviewed: 2026-08-23

Menu of products and libraries relevant to **Scriptorium**: assemble scanned page folders into
textured virtual books (page-turn UX), multi-book libraries, OCR, region hotspots with
background popups, and optional embeddings. **Menu, not mandate** — pick the minimal set.

## Does something like this already exist?

**Pieces exist; the full product mix is uncommon as one open app.**

| Need | What exists today | Gap vs Scriptorium |
|---|---|---|
| Flipbook / page-turn over PDF or images | Commercial flipbook SaaS; OSS widgets (elk-flipbook, StPageFlip wrappers) | Usually marketing PDFs, not curator authoring + multi-book library |
| Deep-zoom manuscript + annotations | IIIF stack (Mirador, Universal Viewer, OpenSeadragon + Annotorious) | Scholarly UX (pan/zoom), not “physical book” texture/turn by default |
| Curator annotate → publish static exhibit | [Archie](https://github.com/micahchoo/Archie) | Strong for exhibits/hotspots; not primarily a book-turn metaphor |
| Hotspots on media | ThingLink and similar commercial tools | Vendor lock-in; not scan-folder → book pipeline |
| OCR + search on scans | Tesseract, PaddleOCR, cloud OCR; elk-flipbook OCR fallback | Authoring + library + long-press lore popups still custom |

**Practical takeaway:** reuse **OCR** + **annotation/region** libraries; own the page turn (Scriptorium
chose its own WebGL2 engine over page-flip, see [ADR-0001](../docs/adr/0001-flipbook-engine.md)), the
**authoring model** (books, regions, lore), **library**, and **ingest** (folder → ordered pages).

Two product architectures both fit; choose deliberately later:

1. **Flipbook-first** — a page-turn renderer over pre-rendered page images; overlays for hotspots.
   Scriptorium took this path with its own WebGL2 engine ([ADR-0001](../docs/adr/0001-flipbook-engine.md)).
2. **IIIF-first** — Presentation manifests + OpenSeadragon/Mirador-class viewers; book-mode paging;
   W3C Web Annotations for regions. Better for GLAM interoperability; weaker “paper book” feel
   unless you layer a flip UI on top.

## Page turn / flipbook

| Item | Role | Notes |
|---|---|---|
| [StPageFlip](https://github.com/Nodlik/StPageFlip) (`page-flip`) | Page-turn candidate | Last npm publish ~2022; **fixed width/height**. Rejected for Scriptorium by [ADR-0001](../docs/adr/0001-flipbook-engine.md). |
| [elk-flipbook](https://github.com/kokiddp/elk-flipbook) | PDF → flipbook + search + optional Tesseract OCR | Built on pdf.js + StPageFlip |
| [read-as-book](https://github.com/Ethical-Tech-CoLab/read-as-book) | Pre-render PDF→images CLI + viewer | Fast static hosting; no pdf.js in browser |
| [PDFlipbook](https://github.com/SympleNZ/PDFlipbook) / [PageFlipOpen](https://github.com/philhoyt/PageFlipOpen) | PDF flip viewers | Smaller / newer; evaluate before depending |
| turn.js | Legacy jQuery flip | Unmaintained; not a candidate |

**Texture / material look:** mostly custom CSS/canvas or WebGL (paper grain, lighting, hard
covers). Scriptorium draws these in its own WebGL2 engine. Not usually a single library.

## Deep zoom, annotations, GLAM standards

| Item | Role | Notes |
|---|---|---|
| [OpenSeadragon](https://openseadragon.github.io/) | Deep-zoom tiles | Pair with flipbook for “inspect detail” mode |
| [Annotorious](https://annotorious.github.io/) | Draw/edit regions on images | Hotspots → popup content |
| [IIIF](https://iiif.io/) Presentation + Image APIs | Interop manifests, tiles, annotations | See [awesome-iiif](https://github.com/IIIF/awesome-iiif) |
| [Mirador](https://projectmirador.org/) / Universal Viewer | Full scholarly viewers | Heavy; embed or learn patterns, don’t fork lightly |
| [triiiceratops](https://github.com/d-flood/triiiceratops) | Modern IIIF web component + annotations | Lighter embeddable viewer |
| [Mimir IIIF Explorer](https://github.com/ashtree4711/mimir-iiif-explorer) | Book/continuous modes + OCR overlays (ALTO/hOCR) | Good reference for OCR overlay UX |
| [Archie](https://github.com/micahchoo/Archie) | Studio + static viewer, Annotorious, IIIF export | Closest open “curator authoring” cousin |
| W3C Web Annotation | Portable region + body model | Prefer over proprietary hotspot JSON long-term |

## OCR, layout, image extraction

| Item | Role | Notes |
|---|---|---|
| [Tesseract](https://github.com/tesseract-ocr/tesseract) / [tesseract.js](https://github.com/naptha/tesseract.js) | OCR | Local; js for in-browser, native/CLI for batch quality |
| [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) | Strong layout/OCR (esp. complex pages) | Heavier Python stack |
| [ocrmypdf](https://github.com/ocrmypdf/OCRmyPDF) | PDF OCR pipeline | If ingest is PDF-centric |
| pdf.js | Render PDF pages → canvas/images | Bridge if sources are PDFs not folders |
| OpenCV / Pillow | Crop, deskew, extract figures | Figure detection often custom or ML |
| ALTO / hOCR | OCR coordinate formats | Feed searchable overlays / IIIF text |

## Embeddings (clarify intent)

“Embedding” may mean **(A)** assets embedded in the book package, or **(B)** vector embeddings
for semantic search over OCR/regions.

| If meaning | Candidates |
|---|---|
| A — package assets | Book as folder/zip: pages/, ocr/, annotations.json, media/ |
| B — vectors | Local: `sqlite-vec`, LanceDB, Chroma; models: open CLIP (image regions) + text embeddings on OCR |

## App shell (web + standalone)

| Item | Role | Notes |
|---|---|---|
| Vite + TypeScript + React/Svelte | Web app | Flipbook libs are browser-native |
| [Tauri](https://tauri.app/) | Desktop wrapper + local folder access | Lighter than Electron; good for librarian local ingest |
| PWA | Installable web without store | Offline/cache strategy needed for large page images |
| Capacitor | Optional iOS/Android shell | Later if kiosk/tablet matters |

## Commercial cousins (evaluate, don’t depend)

FlipHTML5, Heyzine, Issuu-style flipbooks; ThingLink for hotspots; museum virtual-tour tools
(Panoee, etc.). Useful UX references; not a substitute for a GLAM-oriented open product.

## Suggested adoption for Scriptorium (draft)

| Now (research / spike) | Later | Skip for now |
|---|---|---|
| Own WebGL2 page turn ([ADR-0001](../docs/adr/0001-flipbook-engine.md)) + folder-of-images ingest | IIIF export / OpenSeadragon detail mode | Full Mirador fork |
| Annotorious or simple SVG/`xywh` hotspots | W3C Annotation export | Commercial flipbook SaaS |
| Tesseract batch OCR → searchable text + region hints | PaddleOCR if quality insufficient | Cloud OCR unless policy allows |
| Static book package format (JSON + assets) | Vector search if product needs it | Symphony / heavy agent frameworks |

Update this file as spikes settle; durable choices also go in `.context/project-profile.md`.
