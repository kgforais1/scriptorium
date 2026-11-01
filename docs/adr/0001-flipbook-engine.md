# ADR-0001: Page-turn engine — port mockup E's WebGL2 curl engine

Last reviewed: 2026-10-30
Date: 2026-10-28
Status: accepted
Deciders: product owner, Claude Code (pre-M1 spike)

## Context

M1 needs a page-turn reader for scanned books. The reader look is mockup E, Lamplit gallery
([`design/DESIGN.md`](../../design/DESIGN.md)), and every skin is data only. The
[pre-M1 spike plan](../../plans/archive/completed/2026-08-23-pre-m1-flipbook-spike.md) set six gates (G1–G6)
that the chosen engine must pass before M1 starts.

StPageFlip (`page-flip` / `react-pageflip`) was the prior default. It last published around
2022, needs one fixed page size for the whole book, and cannot draw E's lamp, curl shading, or
cover materials. E's own engine already draws all of that, but it was prototype code: fixed page
count, procedural pages, no teardown.

## Decision

We will port mockup E's WebGL2 engine to TypeScript as our own page-turn engine. We will not use
a page-flip library.

- **Library:** none. Own code, ported from
  [`design/mockups/e-lamplit-gallery-ui.html`](../../design/mockups/e-lamplit-gallery-ui.html).
  The spike port was rewritten as [`src/features/reader/engine/`](../../src/features/reader/engine/)
  in M1 Phase 3. The spike itself is in git history before that removal.
- **Canvas vs HTML:** neither StPageFlip mode applies. The book is one WebGL2 canvas.
  Hard pages for M7 are a geometry change in the same engine (a stiff leaf with no bend), not
  a library limit.
- **React:** 19.3.1 under `React.StrictMode`. Each mount creates one engine and the effect
  cleanup disposes it. The async start is abortable, so a StrictMode double mount never leaves
  two engines on one canvas.
- **Aspect policy: pad-to-uniform.** Page 1 fixes the book's page aspect. Every other scan is
  fitted inside that page box with no stretch and no crop. The leftover band is filled with the
  skin's `material.paper` colour.
- **Spread model:** slot 0 is a blank front pastedown, slot *i* is scan *i − 1*, and a blank back
  pastedown pads the slot count to even. The first scan opens alone on the right, like a real book.
- **Non-outcome:** there is no fallback to a non-flip `<img>` strip. If WebGL2 is missing, the
  reader shows an error. M1 must keep that behaviour unless the product owner decides otherwise.

### `scene.json` keys the engine reads

All keys are required. A missing or malformed key is a load error
([`scene.ts`](../../src/features/reader/engine/scene.ts)).

| Key | Meaning |
|---|---|
| `skin` | Skin id |
| `camera.fovDegrees` | Vertical field of view, 1–120 |
| `background.srgb` | Clear colour behind the desk |
| `light.type` | `point` or `directional` |
| `light.color` | Linear HDR light colour |
| `light.position`, `light.poolCenter` | Point light only: lamp position and the centre of its pool on the desk |
| `light.direction` | Directional light only: non-zero vector toward the scene |
| `ambient` | Linear HDR ambient colour |
| `exposure` | Tone-map exposure |
| `vignette` | Corner darkening, 0 (none) to 1 (black corners) |
| `material.coverLeather.dark`, `.light` | Cover leather tones |
| `material.gilt` | Gilt tooling on the cover |
| `material.ribbon` | Ribbon bookmark |
| `material.pageEdge` | Page-block edges |
| `material.paper` | Letterbox band around scans, and blank pastedowns |
| `material.desk.light`, `.mid`, `.dark` | Procedural wood desk tones |

The spike added `vignette`, `material.paper`, and `material.desk` to
[`design/skins/lamplit/scene.json`](../../design/skins/lamplit/scene.json). Material colours
are sRGB and the shader linearizes them.

## Gate results

Tested on macOS (Apple silicon) on 2026-10-28. "Chromium" is Playwright's Chrome for Testing
with Metal ANGLE. "WKWebView" is the Tauri dev window.

| # | Gate | Result |
|---|---|---|
| G1 | Metaphor | **Pass.** The page curls on the keyboard (WKWebView, Chromium), on mouse drag (WKWebView, Chromium), and on one-finger touch drag (Chromium touch emulation over CDP, `pointerType` touch). Touch was not tested on real hardware. Hover peek is mouse only. |
| G2 | Heterogeneous | **Pass.** Fixture pages with aspects 0.69, 0.72, 1.48 (landscape) and 1.0 (square) display letterboxed with no stretch. |
| G3 | StrictMode | **Pass.** The dev double mount, plus a manual dispose-and-recreate button, leave a working book in both webviews. |
| G4 | Asset load | **Pass on macOS.** Pages load from disk via `convertFileSrc` and the asset protocol. `fetch` of `asset://` URLs works from the dev origin. Windows and Linux are not tested. |
| G5 | ADR | **Pass.** This document. |
| G6 | Skin-ready | **Pass.** Swapping to [`design/skins/daylight/scene.json`](../../design/skins/daylight/scene.json) at runtime changes the light (to directional), background, cover, ribbon, paper, and desk with no code change. |

Interaction requirements from the plan:

- A cancelled touch drag (`touchcancel` → `pointercancel`) settles the page, and the next turn
  works. Lost pointer capture takes the same path.
- Navigation during a turn animation, or while a corner peek settles back, is queued (up to
  two steps) and applied when the turn ends. Navigation during an active drag is dropped.
- The hotspot items wait for M6.

## Known limits and measurements

- **Max pages smoke-tested:** 202 (Dulac corpus, 102 spreads) in WKWebView. Twenty-five rapid
  turns kept the cache at its cap.
- **Texture memory:** the height of a page texture is the smallest of the display height in
  device pixels, 2048, the memory budget's limit, the GPU's `MAX_TEXTURE_SIZE`, and the height at
  which the width still fits it. The budget is 256 MB for 12 cached pages with mipmaps, which
  gives `floor(sqrt(TEX_BUDGET * boxAspect / 64))` (`boxAspect` is height over width). A page
  costs about 15 MB at 2048 px. After a resize, an uploaded texture is rebuilt when the new
  height is more than 1.25 times the height it was built at. The old texture stays on screen
  until the new one lands. A smaller target never rebuilds. The LRU keeps 12 textures around the open spread. The
  open spread and the turning leaf are never evicted.
- **Decoding:** pages decode and letterbox in a worker (`createImageBitmap` plus
  `OffscreenCanvas`). Where a worker has no `OffscreenCanvas`, decoding runs on the main thread
  behind the same interface, so an older webview is slower, not broken. The macOS floor is 13.3.
- **Reduced motion:** with `prefers-reduced-motion`, a turn is a short cross-fade and the corner
  peek and pulses stop. The setting is read live, so changing it mid-session takes effect.
- **Context loss:** the engine stops on `webglcontextlost`, and rebuilds its programs and the
  open spread on `webglcontextrestored`.
- **Upload cost:** fetch, decode, letterbox, and upload average 16–37 ms per page in Chromium
  and 51–65 ms in WKWebView.
- **Frame rate:** 120 fps mean (p95 frame 8.9 ms) over ten turns in Chromium on a 120 Hz display.
  WKWebView was not instrumented. The turns looked smooth.
- **Process RSS:** not measured. M1's ≤ 2 GB target still needs a real sample.
- **WebGL2 support:** it works in WKWebView on macOS. WebView2 (Windows) and WebKitGTK (Linux)
  are untested. WebKitGTK is the main risk. Without WebGL2, the reader shows an error state
  and has no fallback.

## M1 rewrite scope

Done in M1 Phase 3. The spike kept its geometry, shaders, and scene loader. The rest was
rewritten. Each item landed as follows:

- The dev-only spike entry in `src/App.tsx` is gone. `App.tsx` renders the reader (chunk 5).
- The code lives in `src/features/reader/`, with E's chrome (header, bottom bar with a basic
  page scrubber, notes panel shell) driven by `--sk-*` tokens from `tokens.json` (chunks 3
  and 4). A thumbnail scrubber waits for M7.
- Pages come from the book package (`meta.json`) through the asset protocol. `spike_pages` and
  the rest of the spike are removed. The asset scope admits only the page files `load_book`
  verified (Phase 2 and chunk 5).
- Decoding runs in a worker, and the texture height follows the display and a memory budget
  (chunk 2).
- `webglcontextlost` and `webglcontextrestored` are handled, and every upload is checked for GL
  errors, including the blank-paper texture (chunk 1).
- The canvas has a visible focus ring and an accessible name that announces the open pages.
  `goTo` is on the controller, and the scrubber uses it (chunks 1 and 4).
- `parseScene`, `fitRect`, and the slot mapping have unit tests (chunk 1).

## Consequences

### Positive

- The lit, curling book from the chosen mockup ships as designed, and skins restyle it through data.
- There is no unmaintained third-party flip library to fork or patch.
- Page size comes from the scans, so non-uniform books work.

### Negative / tradeoffs

- We own about 1,200 lines of WebGL and geometry code, and its bugs.
- WebGL2 is a hard requirement. There is no DOM fallback by design.
- Cross-platform webview support is unproven until M1 runs on Windows and Linux.

### Neutral

- `scene.json` keys are now fixed. Renaming one changes every skin.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| StPageFlip / `page-flip` as-is | Unmaintained since about 2022. It needs one fixed page size, and it cannot draw E's lamp, shading, or materials. |
| Fork `page-flip` | It still could not draw E's scene. We would maintain a fork and still need our own renderer. |
| HTML-mode StPageFlip or own CSS 3D turn | Flat CSS transforms cannot draw a curled, lit page. It would also fail the skin model's light and material gate (G6). |
| Survey other libraries | E already had a working engine for the exact look chosen. The plan bounded the search, and porting was cheaper than shopping. |

## Demo

```sh
pnpm tauri dev    # WKWebView, pages via the asset protocol; opens the last book, or Home
pnpm dev          # browser only, http://localhost:1420 (no Tauri bridge, so no book opens)
pnpm shots        # headless Chrome with a stubbed bridge; screenshots land in tmp/shots/
```

The reader is the app: open a book from Home and it shows the flipbook full-screen. `pnpm tauri dev`
starts its own Vite on port 1420, so stop any other `pnpm dev` first. `pnpm shots` needs the
Dulac corpus or the fixture book in `tmp/` (see `tests/fixtures/README.md`).

Arrow keys or Page Up / Page Down turn the page, and Home and End jump to the ends. Drag a page
to turn it by hand. The bottom bar's scrubber jumps to a spread, and **Notes** opens the notes
panel. The engine's `stats()` reports the texture height, but no screen shows it.

## References

- [Pre-M1 spike plan](../../plans/archive/completed/2026-08-23-pre-m1-flipbook-spike.md)
- [Reader design and skin model](../../design/DESIGN.md)
- [M1 scan viewer plan](../../plans/2026-08-23-m1-scan-viewer.md)
