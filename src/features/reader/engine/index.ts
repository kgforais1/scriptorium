// Public surface of the flipbook engine. Chrome and the reader import from here.

export { FlipbookEngine } from "./engine";
export type { EngineChange, EngineError, EngineOptions, EngineStats, FlipbookController } from "./engine";
export { GEOMETRY_SLOTS, GEOMETRY_STRIDE } from "./corners";
export type { ComposeTarget, DecodedPage, PageDecoder, PageSource } from "./decode";
export { parseScene, SceneError, type Scene } from "./scene";
export { firstPageOfSpread, pagesOfSpread, spreadCount, spreadOfPage } from "./spreads";
