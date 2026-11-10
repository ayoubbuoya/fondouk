import { describe, expect, it } from 'vitest'

import { HUB, TILE_PX } from '../game/medinaLayout'
import { facingAngle, rectCentre, toGround } from './coords'

describe('toGround', () => {
  it('turns one tile of layout pixels into one world unit', () => {
    expect(toGround({ x: TILE_PX, y: 2 * TILE_PX })).toEqual({ x: 1, z: 2 })
  })

  it('puts the hub where the layout says, in tiles', () => {
    expect(toGround(HUB)).toEqual({ x: HUB.x / TILE_PX, z: HUB.y / TILE_PX })
  })

  it('keeps fractions (characters stand between tiles)', () => {
    expect(toGround({ x: 16, y: 48 })).toEqual({ x: 0.5, z: 1.5 })
  })
})

describe('rectCentre', () => {
  it('is the middle of the rectangle', () => {
    expect(rectCentre({ x: 10, y: 4, w: 4, h: 3 })).toEqual({ x: 12, z: 5.5 })
  })
})

describe('facingAngle', () => {
  it('faces the camera (no turn) when walking south', () => {
    expect(facingAngle(0, 10)).toBeCloseTo(0)
  })

  it('makes a quarter turn when walking east or west', () => {
    expect(facingAngle(10, 0)).toBeCloseTo(Math.PI / 2)
    expect(facingAngle(-10, 0)).toBeCloseTo(-Math.PI / 2)
  })

  it('turns around when walking north', () => {
    expect(Math.abs(facingAngle(0, -10))).toBeCloseTo(Math.PI)
  })
})
