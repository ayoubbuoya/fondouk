import { describe, expect, it } from 'vitest'

import { HUB, PAVED, WORLD_PIXELS, WORLD_TILES } from './medinaLayout'
import { buildGroundTiles, pavedMask, pixelToTile, TILES } from './townMap'

const SAND = new Set<number>([TILES.sand, ...TILES.sandDetails])
const PAVING = new Set<number>([...TILES.paved.flat(), ...TILES.pavedDetails])

describe('buildGroundTiles', () => {
  const tiles = buildGroundTiles()
  const mask = pavedMask()

  it('covers the whole world', () => {
    expect(tiles).toHaveLength(WORLD_TILES.h)
    for (const row of tiles) expect(row).toHaveLength(WORLD_TILES.w)
  })

  it('uses paving tiles exactly where the layout paves, sand elsewhere', () => {
    for (let ty = 0; ty < WORLD_TILES.h; ty++) {
      for (let tx = 0; tx < WORLD_TILES.w; tx++) {
        const expected = mask[ty][tx] ? PAVING : SAND
        expect(expected.has(tiles[ty][tx]), `tile ${tx},${ty}`).toBe(true)
      }
    }
  })

  it('paves the hub with a centre tile (paving on all 4 sides)', () => {
    const { tx, ty } = pixelToTile(HUB.x, HUB.y)
    expect([TILES.paved[1][1], ...TILES.pavedDetails]).toContain(tiles[ty][tx])
  })

  it('draws the corners of a paved area with corner tiles', () => {
    // The court's own corners are not touched by any street.
    const court = PAVED[0]
    expect(tiles[court.y][court.x]).toBe(TILES.paved[0][0])
    expect(tiles[court.y + court.h - 1][court.x + court.w - 1]).toBe(TILES.paved[2][2])
  })

  it('has no border where the caravan road leaves the map', () => {
    // The north-south street is 2 tiles wide: its last row has side borders only.
    const road = PAVED[2]
    const lastRow = WORLD_TILES.h - 1
    expect(tiles[lastRow][road.x]).toBe(TILES.paved[1][0])
    expect(tiles[lastRow][road.x + 1]).toBe(TILES.paved[1][2])
  })

  it('gives the same map every time', () => {
    expect(buildGroundTiles()).toEqual(tiles)
  })
})

describe('pixelToTile', () => {
  it('clamps to the world', () => {
    expect(pixelToTile(-5, -5)).toEqual({ tx: 0, ty: 0 })
    expect(pixelToTile(WORLD_PIXELS.w + 50, WORLD_PIXELS.h + 50)).toEqual({
      tx: WORLD_TILES.w - 1,
      ty: WORLD_TILES.h - 1,
    })
  })
})
