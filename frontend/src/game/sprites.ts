/**
 * The characters a player can pick (F3.1, HANDOFF D54) and how to animate them.
 *
 * **Keep in sync** with `SPRITES` in `backend/src/api/avatars.rs`: the backend
 * refuses any other id, and this file holds the art for each one.
 *
 * Art: Ninja Adventure pack, CC0 (`public/assets/CREDITS.md`). Each
 * `walk.png` is 64×64 = 4 columns × 4 rows of 16 px frames:
 *
 * ```text
 *            col 0   col 1   col 2   col 3
 *            down    up      left    right
 * row 0      0       1       2       3      <- also the standing frame
 * row 1      4       5       6       7
 * row 2      8       9       10      11
 * row 3      12      13      14      15
 * ```
 */

export const SPRITES = [
  'boy',
  'princess',
  'old_man',
  'hunter',
  'monk',
  'noble',
  'samurai',
  'ninja_blue',
] as const

export type SpriteId = (typeof SPRITES)[number]

/** Shown for any id the frontend does not know (older or newer data). */
export const DEFAULT_SPRITE: SpriteId = 'boy'

export type Direction = 'down' | 'up' | 'left' | 'right'

const DIRECTION_COLUMN: Record<Direction, number> = { down: 0, up: 1, left: 2, right: 3 }
const COLUMNS = 4
const ROWS = 4

/** A known sprite id, or the default one. */
export function knownSprite(sprite: string): SpriteId {
  return (SPRITES as readonly string[]).includes(sprite) ? (sprite as SpriteId) : DEFAULT_SPRITE
}

/** Phaser texture key of a character's walk sheet. */
export function walkTextureKey(sprite: SpriteId): string {
  return `walk-${sprite}`
}

/** Phaser animation key for walking one way. */
export function walkAnimKey(sprite: SpriteId, direction: Direction): string {
  return `walk-${sprite}-${direction}`
}

export function walkSheetUrl(sprite: SpriteId): string {
  return `/assets/characters/${sprite}/walk.png`
}

/** The 38×38 portrait used by the side panel. */
export function faceUrl(sprite: string): string {
  return `/assets/characters/${knownSprite(sprite)}/face.png`
}

/** The 4 frames of walking in one direction, top to bottom of its column. */
export function walkFrames(direction: Direction): number[] {
  const column = DIRECTION_COLUMN[direction]
  return Array.from({ length: ROWS }, (_, row) => row * COLUMNS + column)
}

/** The frame shown while standing still (first walk frame). */
export function standFrame(direction: Direction): number {
  return DIRECTION_COLUMN[direction]
}

/**
 * Which way to face for a step of (dx, dy). The larger axis wins; on an
 * exact diagonal (our roads are 45°) up/down wins, which reads better.
 */
export function directionOf(dx: number, dy: number): Direction {
  if (Math.abs(dx) > Math.abs(dy)) return dx < 0 ? 'left' : 'right'
  return dy < 0 ? 'up' : 'down'
}
