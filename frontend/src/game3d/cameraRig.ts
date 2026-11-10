/**
 * The numbers behind the 3D camera. Pure, no three.js: unit-tested.
 * `createTown3d.ts` feeds them to three's `MapControls`.
 *
 * ```text
 *   side view                         "tilt" = angle from straight down
 *        camera ◉                       0      = a flat map from above
 *              ╱ ╲  fov                 π / 2  = standing on the ground
 *    distance ╱   ╲
 *            ╱tilt ╲
 *   ────────●───────────  ground      ● = the point the camera looks at
 * ```
 */

/** Vertical field of view of the camera, in degrees. */
export const FOV_DEG = 40

/** How far the user may tilt, in radians from straight down. */
export const TILT_LIMITS = { min: 0.3, max: 1.15 } as const
/** The tilt the town opens with: clearly 3D, but every district sign readable. */
export const START_TILT = 0.8

/** The camera never comes closer to the ground point it looks at than this (units = tiles). */
export const MIN_DISTANCE = 9
/** The user may zoom out a little past the opening view, to see the desert. */
const MAX_DISTANCE_FACTOR = 1.35
/** Empty space kept around the town in the opening view. */
const FIT_MARGIN = 1.06

/** Distance used when the Judge Tour focuses on one building or character. */
export const FOCUS_DISTANCE = 22

export interface ViewSize {
  w: number
  h: number
}

/** Screen pixels covered by the HUD bar (top) and the news strip (bottom). */
export interface Insets {
  top: number
  bottom: number
}

/**
 * The camera distance at which a `town.w` × `town.d` rectangle (in units)
 * fits in the part of the view no bar covers.
 *
 * Two things can be the tight one: the town's width against the view's
 * width, or its depth against the free height. Seen at an angle, the depth
 * looks shorter by `cos(tilt)`. The larger distance wins.
 */
export function fitDistance(town: { w: number; d: number }, view: ViewSize, insets: Insets, tilt: number): number {
  if (view.w <= 0 || view.h <= 0) return MIN_DISTANCE
  const halfHeightAtOne = Math.tan(((FOV_DEG / 2) * Math.PI) / 180)
  const aspect = view.w / view.h
  // Never let the bars take the whole view (a very short window).
  const freeShare = Math.max(0.2, (view.h - insets.top - insets.bottom) / view.h)

  const forWidth = town.w / 2 / (halfHeightAtOne * aspect)
  const forDepth = ((town.d / 2) * Math.cos(tilt)) / (halfHeightAtOne * freeShare)
  return Math.max(MIN_DISTANCE, Math.max(forWidth, forDepth) * FIT_MARGIN)
}

/** How close and how far the user may zoom, given the opening distance. */
export function distanceLimits(startDistance: number): { min: number; max: number } {
  return { min: Math.min(MIN_DISTANCE, startDistance), max: startDistance * MAX_DISTANCE_FACTOR }
}

/** Keeps the point the camera looks at inside a ground rectangle, so the user cannot pan into the void. */
export function clampToRect(
  x: number,
  z: number,
  rect: { x: number; z: number; w: number; d: number },
): { x: number; z: number } {
  return {
    x: Math.min(rect.x + rect.w, Math.max(rect.x, x)),
    z: Math.min(rect.z + rect.d, Math.max(rect.z, z)),
  }
}

/** Smooth start and stop for camera moves: 0 -> 0, 1 -> 1, slow at both ends. */
export function easeInOut(t: number): number {
  const clamped = Math.min(1, Math.max(0, t))
  return clamped * clamped * (3 - 2 * clamped)
}
