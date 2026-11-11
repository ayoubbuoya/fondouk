/**
 * The small motions of the 3D town (D80, D81). Pure, no three.js: unit-tested.
 * `effects.ts` (coins, yield glow) and `ambient.ts` (palms, banners, smoke,
 * fountain) turn these numbers into moving shapes.
 *
 * ```text
 *   arcPoint      a point flying from A to B over a hump       coins, fountain drops
 *   apyGlowLevels how strongly each village's ring glows (0..1)  yield glow
 *   swayAngle     a slow back-and-forth turn                    palms, banners
 *   smokePuff     how far one puff has risen, its size, its fade  chimney smoke
 * ```
 */

import { totalApy } from '../lib/format'
import type { Village } from '../lib/api'

/** A point in the 3D world, y up (tiles). */
export interface WorldPoint {
  x: number
  y: number
  z: number
}

/**
 * Where something flying from `from` to `to` is at share `t` (0..1) of its
 * flight: a straight line, lifted by a hump that is `height` high in the middle.
 *
 * ```text
 *        height ┬     .  ·  .
 *               │  ·           ·
 *   from ●──────┴─────────────────● to     (the line between them may slope)
 *        t = 0        t = 0.5      t = 1
 * ```
 */
export function arcPoint(from: WorldPoint, to: WorldPoint, height: number, t: number): WorldPoint {
  const clamped = Math.min(1, Math.max(0, t))
  // 4·t·(1 − t) is 0 at both ends and exactly 1 in the middle.
  const lift = height * 4 * clamped * (1 - clamped)
  return {
    x: from.x + (to.x - from.x) * clamped,
    y: from.y + (to.y - from.y) * clamped + lift,
    z: from.z + (to.z - from.z) * clamped,
  }
}

/**
 * How strongly each village's ground ring glows, from 0 (none) to 1 (the best
 * yield in town). Relative on purpose (D80): the ring answers "where is the
 * yield right now", the sign above it gives the exact number.
 *
 * A stale village, one without an APY yet, or one at 0 % or below gets 0.
 * Uses `totalApy`, like the signs, so ring and number always agree.
 */
export function apyGlowLevels(
  villages: readonly Pick<Village, 'id' | 'apy_base' | 'apy_reward' | 'stale'>[],
): Map<string, number> {
  const apys = new Map<string, number>()
  for (const village of villages) {
    const apy = village.stale ? null : totalApy(village)
    apys.set(village.id, apy !== null && Number.isFinite(apy) && apy > 0 ? apy : 0)
  }
  const best = Math.max(0, ...apys.values())
  const levels = new Map<string, number>()
  // `best` is 0 when no village has a positive APY: no rings, and no division by 0.
  for (const [id, apy] of apys) levels.set(id, best > 0 ? apy / best : 0)
  return levels
}

/**
 * A slow back-and-forth turn, in radians. `phase` (radians) lets two palms
 * side by side move out of step, so the town does not sway like one machine.
 */
export function swayAngle(seconds: number, phase: number, amplitude: number, period: number): number {
  return amplitude * Math.sin((seconds / period) * Math.PI * 2 + phase)
}

/** How one smoke puff looks over its life. */
export interface PuffLook {
  /** Height above the chimney (tiles). */
  rise: number
  /** Size compared to a puff just out of the chimney. */
  scale: number
  /** 0 = invisible, 1 = solid. */
  opacity: number
}

const PUFF = { rise: 2.6, startScale: 0.35, growth: 1.1, maxOpacity: 0.5, fadeIn: 0.12 } as const

/**
 * A smoke puff `age` seconds after it left the chimney, for a puff that lives
 * `life` seconds. It rises and grows the whole time, fades in quickly at the
 * start (so it does not pop into view) and fades out to nothing at the end
 * (so it can be reused at the chimney without a jump).
 */
export function smokePuff(age: number, life: number): PuffLook {
  const t = Math.min(1, Math.max(0, age / life))
  return {
    rise: PUFF.rise * t,
    scale: PUFF.startScale + PUFF.growth * t,
    opacity: PUFF.maxOpacity * Math.min(1, t / PUFF.fadeIn) * (1 - t),
  }
}

/** The shortest and longest wait between two gestures of a standing merchant (seconds). */
export const GESTURE_WAIT = { min: 8, max: 20 } as const

/** Seconds until a standing merchant's next gesture, from a random number 0..1 (`Math.random()`). */
export function idleGestureDelay(random: number): number {
  const clamped = Math.min(1, Math.max(0, random))
  return GESTURE_WAIT.min + (GESTURE_WAIT.max - GESTURE_WAIT.min) * clamped
}
