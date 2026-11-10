/**
 * Which town is shown: the 2D Phaser one (default) or the 3D one (D76).
 * The 3D town is opt-in while it is reviewed: add `?view=3d` to the address.
 *
 * One rule, read by the town itself (`ui/TownCanvas.tsx`) and by the
 * portraits (`game/sprites.ts` `faceUrl`), so the faces in the panels always
 * match the characters in the town.
 */

/**
 * @param search the address's query string; defaults to the current page's
 *   (`""` outside a browser, e.g. in unit tests)
 */
export function isTownView3d(search: string = typeof window === 'undefined' ? '' : window.location.search): boolean {
  return new URLSearchParams(search).get('view') === '3d'
}
