import { describe, expect, it } from 'vitest'

import { CHARACTER_MODELS, faceUrl, modelUrl, SPRITE_LOOKS, SPRITES } from './sprites'

describe('faceUrl', () => {
  it('shows the pixel face in the 2D town', () => {
    expect(faceUrl('monk', false)).toBe('/assets/characters/monk/face.png')
  })

  it('shows the 3D headshot in the 3D town', () => {
    expect(faceUrl('monk', true)).toBe('/assets/characters/monk/face3d.png')
  })

  it('falls back to the default character for an unknown id, in both towns', () => {
    expect(faceUrl('merchant_9', false)).toBe('/assets/characters/boy/face.png')
    expect(faceUrl('merchant_9', true)).toBe('/assets/characters/boy/face3d.png')
  })

  it('defaults to the 2D face outside a browser', () => {
    expect(faceUrl('monk')).toBe('/assets/characters/monk/face.png')
  })
})

describe('SPRITE_LOOKS (3D town)', () => {
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
