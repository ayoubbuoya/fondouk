/**
 * The characters a player can pick (F3.1, HANDOFF D54) and how each one looks
 * in the 3D town (D76).
 *
 * **Keep in sync** with `SPRITES` in `backend/src/api/avatars.rs`: the backend
 * refuses any other id. The ids are only keys now: their names come from the
 * first, 2D art (HANDOFF D77 removed it) and are kept so no data has to change.
 *
 * Art: KayKit Adventurers, CC0 (`public/assets/CREDITS.md`).
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

/**
 * The 3D bodies (`game3d/characters.ts`). There are fewer models than sprite
 * ids, so some ids share a model and differ by colour.
 * Files: `public/assets/models/characters/<model>.glb`.
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

/** A known sprite id, or the default one. */
export function knownSprite(sprite: string): SpriteId {
  return (SPRITES as readonly string[]).includes(sprite) ? (sprite as SpriteId) : DEFAULT_SPRITE
}

/** The 128×128 headshot the panels show, rendered from the 3D model (`game3d/portraitStudio.ts`). */
export function faceUrl(sprite: string): string {
  return `/assets/characters/${knownSprite(sprite)}/face.png`
}
