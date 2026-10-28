# Design: Reader look and skins

Last reviewed: 2026-10-27
Date: 2026-10-27
Author: product owner + Claude Code
Status: approved
Supersedes: n/a

## Decision

The reader follows **mockup E, Lamplit gallery**
([`mockups/e-lamplit-gallery-ui.html`](mockups/e-lamplit-gallery-ui.html)). It pairs the
lamplit desk scene from A with the gallery chrome from B: a floating bottom bar with a
thumbnail scrubber, a header with tools, and a right-hand notes panel.

The reader supports **multiple skins** from the start. Lamplit is the first and default skin.
Gallery Daylight (mockup B) is the planned second skin. B shares E's layout and most of its CSS
variable names. B's WebGL scene uses window light, not a lamp. So `scene.json` has a typed light: `point`
for a lamp with a pool, or `directional` for window light. The engine must support both
before M1 starts (spike gate G6). A directional light has `type`, `direction` (a unit
vector toward the scene, such as `[-0.3, -0.8, -0.5]`), and `color`. It has no `position` or
`poolCenter`. All other scene keys are shared by both types.

Mockups stay reference only. Porting E's engine means re-implementing its curl and lighting
against scan textures, not copying the mockup file.

## Principles

- The scan is the subject. The scene frames the page and never alters scan pixels beyond
  lighting. No stretch, no crop (see the aspect policy in the spike ADR).
- The book feels physical. Pages curl, cast shadows, and catch the lamp.
- Chrome recedes. Bars and panels are translucent glass, fade during turns, and never cover
  the gutter.
- Notes are opt-in. Hotspot markers can be hidden with one toggle.
- Motion respects `prefers-reduced-motion`. Pulses stop and turns shorten.
- Every control works by keyboard and is named for assistive technology.
- Touch is a first-class input. A finger drag turns the page, every control has a touch
  target of at least 44 px, and nothing works only on hover. Hover may add polish but must
  have a tap equivalent.

## Skin model

One page-turn engine and one layout serve every skin. A skin is data only. Adding a skin must
not need engine or component code changes.

| Layer | Shared or per skin | Where it lives |
|---|---|---|
| Page-turn engine (geometry, curl, picking, input) | Shared | App code, chosen by the spike ADR |
| Layout (header, bottom bar, notes panel) | Shared | React components |
| UI theme (colour, type, radius, blur, motion) | Per skin | `skins/<id>/tokens.json` |
| Scene (light, ambient, camera, background) | Per skin | `skins/<id>/scene.json` |
| Book material (cover, gilt, ribbon, page edges) | Per skin | `skins/<id>/scene.json` |

Rules:

- `tokens.json` uses the W3C Design Tokens (DTCG) format. A build step turns it into CSS
  custom properties. Components read only those properties and never hard-code colours.
- `scene.json` is engine input with its own small schema. Its keys are provisional until the
  spike ADR fixes the engine. The ADR may rename them.
- Book size and aspect come from the scans, never from the skin.
- A skin may not change layout. Swappable layouts, such as the B-style scrubber and panel
  versus the A-style toolbar, are out of scope until a real need appears.
- Token names may differ from mockup variable names. For example, E's `--accent2` is the
  `accent-strong` token. Code carried over from a mockup must use the token names.
- Every skin defines the same token and scene keys. A missing key is a load error, not a
  silent fallback.

## Skins

| Id | Source | Status |
|---|---|---|
| `lamplit` | Mockup E | Default. Ships in M1. |
| `daylight` | Mockup B | Planned. Second skin and skin picker in M7. |

## Related

- Spike and engine choice: [`../plans/2026-08-23-pre-m1-flipbook-spike.md`](../plans/2026-08-23-pre-m1-flipbook-spike.md)
- First use in the app: [`../plans/2026-08-23-m1-scan-viewer.md`](../plans/2026-08-23-m1-scan-viewer.md)
- Roadmap: [`../plans/2026-08-23-product-roadmap.md`](../plans/2026-08-23-product-roadmap.md)
