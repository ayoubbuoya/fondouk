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

import { isTownView3d } from '../lib/townView'

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

/**
 * The 3D bodies of the 3D town (`game3d/`, D76). There are fewer models than
 * sprite ids, so some ids share a model and differ by colour.
 * Files: `public/assets/models/characters/<model>.glb` (KayKit Adventurers).
 */
export const CHARACTER_MODELS = ['knight', 'barbarian', 'mage', 'rogue', 'rogue_hooded'] as const

export type CharacterModel = (typeof CHARACTER_MODELS)[number]

export interface CharacterLook {
  model: CharacterModel
  /**
   * Multiplied into the model's colours (white = unchanged), and the robe
   * colour of the stand-in figure shown while a model is missing or loading.
   */
  tint: number
}

/** The 3D look of each sprite id. Two ids on one model always have different tints. */
export const SPRITE_LOOKS: Record<SpriteId, CharacterLook> = {
  boy: { model: 'rogue', tint: 0xffffff },
  princess: { model: 'mage', tint: 0xffc4dd },
  old_man: { model: 'mage', tint: 0xffffff },
  hunter: { model: 'rogue_hooded', tint: 0xffffff },
  monk: { model: 'barbarian', tint: 0xffd9a0 },
  noble: { model: 'knight', tint: 0xffe08a },
  samurai: { model: 'knight', tint: 0xffffff },
  ninja_blue: { model: 'rogue_hooded', tint: 0x8fb6ff },
}

export function modelUrl(model: CharacterModel): string {
  return `/assets/models/characters/${model}.glb`
}

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

/**
 * The portrait used by the panels: the 38×38 pixel face in the 2D town, or
 * the 128×128 3D headshot in the 3D town (`game3d/portraitStudio.ts`), so a
 * face always matches the character on the map.
 *
 * @param view3d defaults to the page's `?view=3d` (`lib/townView.ts`)
 */
export function faceUrl(sprite: string, view3d: boolean = isTownView3d()): string {
  return `/assets/characters/${knownSprite(sprite)}/${view3d ? 'face3d' : 'face'}.png`
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
