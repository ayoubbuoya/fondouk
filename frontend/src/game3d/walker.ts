/**
 * Where a walking character is at a given moment. Pure, no three.js:
 * unit-tested.
 *
 * `walkPath.ts` gives the corners of a walk; the 2D scene handed them to a
 * Phaser tween. The 3D scene has no tweens, so each frame it asks this
 * function "after `elapsed` seconds, where am I and which way do I look?".
 *
 * ```text
 *   from ●────────● path[0]
 *                 │            distance walked = speed × elapsed
 *                 │  ◄ here    -> find the segment that distance falls in
 *                 ● path[1]    -> interpolate inside it
 * ```
 */

import type { Point } from '../game/medinaLayout'

export interface WalkMoment {
  position: Point
  /**
   * The step being walked, as (dx, dy); `null` when there is nothing to walk
   * (empty path). Only its direction matters.
   */
  heading: Point | null
  /** `true` once the last point is reached. */
  done: boolean
}

/**
 * @param from where the walk started
 * @param path the corners to pass, in order (`walkPath`)
 * @param speed layout pixels per second (`walkSpeed`); must be > 0
 * @param elapsedSeconds time since the walk started; negative counts as 0
 */
export function walkAt(from: Point, path: Point[], speed: number, elapsedSeconds: number): WalkMoment {
  let remaining = Math.max(0, elapsedSeconds) * speed
  let start = from
  let heading: Point | null = null

  for (const end of path) {
    const dx = end.x - start.x
    const dy = end.y - start.y
    const length = Math.hypot(dx, dy)
    // A zero-length step has no direction: keep the previous heading.
    if (length > 0) heading = { x: dx, y: dy }
    if (remaining < length) {
      const ratio = remaining / length
      return { position: { x: start.x + dx * ratio, y: start.y + dy * ratio }, heading, done: false }
    }
    remaining -= length
    start = end
  }
  return { position: { ...start }, heading, done: true }
}
