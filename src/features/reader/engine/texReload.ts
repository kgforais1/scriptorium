// When a bigger target texture height should replace textures built smaller. A canvas that was
// laid out tiny (or a window that was small and is later maximised) leaves cached pages blurry,
// and the open spread is pinned so it is never evicted. Pure, so the decision is unit tested.

/** How much taller the target must be than a texture before that texture is rebuilt. */
export const RELOAD_RATIO = 1.25;

/**
 * Whether a texture built at `builtH` is worth rebuilding for `targetH`. Only a clearly larger
 * target counts, so a small resize does not thrash. A smaller target never does.
 */
export function needsReload(builtH: number, targetH: number, ratio = RELOAD_RATIO): boolean {
  return targetH > builtH * ratio;
}

/** The slots among `built` (slot and the height each texture was built at) to rebuild for `targetH`. */
export function slotsToReload(built: Iterable<readonly [slot: number, texH: number]>, targetH: number, ratio = RELOAD_RATIO): number[] {
  const out: number[] = [];
  for (const [slot, texH] of built) if (needsReload(texH, targetH, ratio)) out.push(slot);
  return out;
}
