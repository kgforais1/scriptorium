# ADR-0001: Page-turn engine — port mockup E's WebGL2 curl engine

Last reviewed: 2026-10-28
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
  The spike port lives in [`src/spike/flipbook/`](../../src/spike/flipbook/).
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
([`scene.ts`](../../src/spike/flipbook/scene.ts)).

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
| G6 | Skin-ready | **Pass.** Swapping to [`daylight-placeholder.json`](../../src/spike/flipbook/scenes/daylight-placeholder.json) at runtime changes the light (to directional), background, cover, ribbon, paper, and desk with no code change. |

Interaction requirements from the plan:

- A cancelled touch drag (`touchcancel` → `pointercancel`) settles the page, and the next turn
  works. Lost pointer capture takes the same path.
- Navigation during a turn animation, or while a corner peek settles back, is queued (up to
  two steps) and applied when the turn ends. Navigation during an active drag is dropped.
- The hotspot items wait for M6.

## Known limits and measurements

- **Max pages smoke-tested:** 202 (Dulac corpus, 102 spreads) in WKWebView. Twenty-five rapid
  turns kept the cache at its cap.
- **Texture memory:** pages upload at 2048 px tall with mipmaps (about 15 MB each). A wide book or a
  smaller GPU limit lowers the height so both sides fit `MAX_TEXTURE_SIZE`. The LRU
  keeps 12 textures around the open spread, about 180 MB of VRAM. The open spread and the
  turning leaf are never evicted.
- **Upload cost:** fetch, decode, letterbox, and upload average 16–37 ms per page in Chromium
  and 51–65 ms in WKWebView.
- **Frame rate:** 120 fps mean (p95 frame 8.9 ms) over ten turns in Chromium on a 120 Hz display.
  WKWebView was not instrumented. The turns looked smooth.
- **Process RSS:** not measured. M1's ≤ 2 GB target still needs a real sample.
- **WebGL2 support:** it works in WKWebView on macOS. WebView2 (Windows) and WebKitGTK (Linux)
  are untested. WebKitGTK is the main risk.

## M1 rewrite scope

The spike is throwaway. M1 keeps the geometry, shaders, and scene loader, and rewrites the rest:

- Replace the dev-only spike entry in `src/App.tsx`. Release builds show the placeholder shell.
- Move the code to `src/features/reader/`, and replace the spike harness with E's chrome
  (header, bottom bar with thumbnail scrubber, notes panel) driven by `tokens.json`.
- Feed pages from the book package (`meta.json`) instead of the spike folder command
  (`spike_pages`, debug builds only, which admits only the listed files). Scope the asset
  protocol to the opened book. The `protocol-asset` feature and `assetProtocol.enable` ship in
  every build today with an empty static scope.
- Decode off the main thread (worker plus `OffscreenCanvas`) and pick the texture size from
  the display, not a fixed 2048.
- Handle `webglcontextlost` and `webglcontextrestored`. The spike ignores them.
- Add thumbnail-scrubber `goTo`, a visible focus ring on the canvas, and an accessible name
  that announces the current pages.
- Add unit tests for `parseScene`, `fitRect`, and the slot mapping.

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
scripts/make-spike-fixtures.sh   # needs the Dulac and Princess corpora, see tests/fixtures/README.md
pnpm tauri dev                   # WKWebView, pages via the asset protocol
pnpm dev                         # browser only, http://localhost:1420
SCRIPTORIUM_SPIKE_DIR="$PWD/tmp/fixtures/corpus/dulac-picture-book/jpg" pnpm tauri dev   # 202-page smoke
```

The fixture script needs the Dulac and Princess corpora in `tmp/fixtures/corpus/` (see
`tests/fixtures/README.md`). `pnpm tauri dev` starts its own Vite on port 1420, so stop any
other `pnpm dev` first. The spike shows only in dev builds.

Arrow keys or Page Up / Page Down turn the page. Drag a page to turn it by hand. The bar's
**Scene** button swaps to the daylight placeholder, and **Remount** disposes and recreates the engine.

## References

- [Pre-M1 spike plan](../../plans/archive/completed/2026-08-23-pre-m1-flipbook-spike.md)
- [Reader design and skin model](../../design/DESIGN.md)
- [M1 scan viewer plan](../../plans/2026-08-23-m1-scan-viewer.md)
