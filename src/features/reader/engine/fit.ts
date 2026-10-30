// Pad-to-uniform aspect policy (ADR-0001): each scan is fitted inside the book's page box with
// no stretch and no crop. Pure, so it is unit tested.

/**
 * Where a scan of size w x h sits inside a page box of aspect boxAspect (height over width), as
 * fractions of the box.
 */
export function fitRect(w: number, h: number, boxAspect: number) {
  const scanAspect = h / w;
  if (scanAspect > boxAspect) {
    const fw = boxAspect / scanAspect;
    return { x: (1 - fw) / 2, y: 0, w: fw, h: 1 };
  }
  const fh = scanAspect / boxAspect;
  return { x: 0, y: (1 - fh) / 2, w: 1, h: fh };
}
