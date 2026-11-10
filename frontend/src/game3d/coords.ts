/**
 * From the 2D layout to the 3D world. Pure, no three.js: unit-tested.
 *
 * `medinaLayout.ts` and `walkPath.ts` speak in "layout pixels" (one tile =
 * `TILE_PX` pixels, y grows down the screen). The 3D world uses one unit per
 * tile, with the ground on the X/Z plane and Y pointing up:
 *
 * ```text
 *   layout (pixels)            3D world (units)
 *   ┌────────► x               ┌────────► x
 *   │                          │            y = up, out of the ground
 *   ▼ y                        ▼ z
 * ```
 *
 * So south on the 2D map (larger y) is "towards the camera" (larger z) in 3D.
 */

import { TILE_PX, type Point, type TileRect } from '../game/medinaLayout'

/** A spot on the ground of the 3D world. */
export interface GroundPoint {
  x: number
  z: number
}

/** A layout pixel -> the spot on the 3D ground. */
export function toGround(point: Point): GroundPoint {
  return { x: point.x / TILE_PX, z: point.y / TILE_PX }
}

/** The middle of a tile rectangle, on the 3D ground. Tiles are already world units. */
export function rectCentre(tiles: TileRect): GroundPoint {
  return { x: tiles.x + tiles.w / 2, z: tiles.y + tiles.h / 2 }
}

/**
 * The turn (around the up axis, in radians) that makes a model face the way
 * it walks. Models look along +Z when not turned, so walking south (towards
 * the camera) is 0 and walking east is a quarter turn.
 *
 * @param dx step along x (layout pixels or world units: only the ratio matters)
 * @param dy step along layout y, which is 3D z
 */
export function facingAngle(dx: number, dy: number): number {
  return Math.atan2(dx, dy)
}
