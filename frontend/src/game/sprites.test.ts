import { describe, expect, it } from 'vitest'

import { CHARACTER_MODELS, faceUrl, knownSprite, modelUrl, SPRITE_LOOKS, SPRITES } from './sprites'

describe('knownSprite', () => {
  it('falls back to the default sprite for unknown ids', () => {
    expect(knownSprite('princess')).toBe('princess')
    expect(knownSprite('merchant_1')).toBe('boy')
  })

  it('has 6 to 10 characters to pick from (F3.1)', () => {
    expect(SPRITES.length).toBeGreaterThanOrEqual(6)
    expect(SPRITES.length).toBeLessThanOrEqual(10)
  })
})

describe('faceUrl', () => {
  it('points at the character headshot', () => {
    expect(faceUrl('monk')).toBe('/assets/characters/monk/face.png')
  })

  it('falls back to the default character for an unknown id', () => {
    expect(faceUrl('merchant_9')).toBe('/assets/characters/boy/face.png')
  })
})

describe('SPRITE_LOOKS', () => {
  it('gives every sprite id a known model', () => {
    for (const sprite of SPRITES) {
      expect(CHARACTER_MODELS).toContain(SPRITE_LOOKS[sprite].model)
    }
  })

  it('never gives two sprite ids the same model and tint (they would look identical)', () => {
    const looks = SPRITES.map((sprite) => `${SPRITE_LOOKS[sprite].model}:${SPRITE_LOOKS[sprite].tint}`)
    expect(new Set(looks).size).toBe(SPRITES.length)
  })

  it('uses every model at least once (no unused download)', () => {
    const used = new Set(SPRITES.map((sprite) => SPRITE_LOOKS[sprite].model))
    expect([...used].sort()).toEqual([...CHARACTER_MODELS].sort())
  })
})

describe('modelUrl', () => {
  it('points into the public models folder', () => {
    expect(modelUrl('knight')).toBe('/assets/models/characters/knight.glb')
  })
})
