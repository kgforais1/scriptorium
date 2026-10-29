# Flipbook spike (throwaway)

Prototype for the pre-M1 page-turn spike. It is a TypeScript port of mockup E's WebGL2 engine,
driven by scan textures and a `scene.json`. M1 moves what it keeps to `src/features/reader/` and
deletes this folder. Decisions, measurements, and the rewrite scope are in
[ADR-0001](../../../docs/adr/0001-flipbook-engine.md).

| File | Role |
|---|---|
| `engine.ts` | Engine class: camera, render loop, turning state machine, input |
| `geometry.ts` | Board, page blocks, curling leaf, ribbon (pure, no WebGL) |
| `shaders.ts` | Lit scene and baked desk GLSL; scene values arrive as uniforms |
| `gesture.ts` | Pure hit-area, drag-progress, and release rules |
| `textures.ts` | LRU page textures around the open spread |
| `pages.ts` | Scan decode and the pad-to-uniform letterbox |
| `scene.ts` | `scene.json` parser; a missing key is an error |
| `gl.ts`, `math.ts` | WebGL and matrix helpers |
| `FlipbookSpike.tsx` | React harness (StrictMode-safe mount, scene swap, remount) |
| `scenes/daylight-placeholder.json` | Throwaway directional-light scene for gate G6 |
