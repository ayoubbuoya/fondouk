import { describe, expect, it } from 'vitest'

import { directionOf, knownSprite, SPRITES, walkFrames } from './sprites'
import { buildTownTiles, MAP_TILES, pixelToTile, TILES } from './townMap'
import { arenaSlot, CENTER, layoutStandings, queueSlot, walkPath, type Point } from './walkPath'

const LEND_USDC: Point = { x: 200, y: 200 }
const FRAX: Point = { x: 600, y: 600 }
const villages = new Map<string, Point>([
  ['lend-usdc', LEND_USDC],
  ['frax', FRAX],
])

const key = (point: Point) => `${Math.round(point.x)},${Math.round(point.y)}`

describe('walkPath', () => {
  it('is empty when the character is already there', () => {
    expect(walkPath(LEND_USDC, LEND_USDC, false)).toEqual([])
  })

  it('goes through the arena centre to reach another village', () => {
    expect(walkPath(LEND_USDC, FRAX, false)).toEqual([CENTER, FRAX])
  })

  it('takes one straight step when only the queue spot changes', () => {
    const next = queueSlot(LEND_USDC, 1)
    expect(walkPath(queueSlot(LEND_USDC, 4), next, true)).toEqual([next])
  })

  it('reaches the arena (cash) through the centre as well', () => {
    expect(walkPath(LEND_USDC, arenaSlot(0), false)).toEqual([CENTER, arenaSlot(0)])
  })
})

describe('queueSlot and arenaSlot', () => {
  it('gives a different spot to each of the first 12 in a queue', () => {
    const spots = new Set(Array.from({ length: 12 }, (_, i) => key(queueSlot(LEND_USDC, i))))
    expect(spots.size).toBe(12)
  })

  it('queues towards the arena, never behind the village', () => {
    for (let i = 0; i < 12; i++) {
      const spot = queueSlot(LEND_USDC, i)
      // The top-left village's queue goes down-right, towards the centre.
      expect(spot.x + spot.y).toBeGreaterThan(LEND_USDC.x + LEND_USDC.y)
      expect(spot.x + spot.y).toBeLessThan(CENTER.x + CENTER.y)
    }
  })

  it('gives a different spot to each of the first 16 cash characters', () => {
    const spots = new Set(Array.from({ length: 16 }, (_, i) => key(arenaSlot(i))))
    expect(spots.size).toBe(16)
  })
})

describe('layoutStandings', () => {
  it('puts characters at their village, sorted by id, and cash in the arena', () => {
    const spots = layoutStandings(
      [
        { id: 'b', villageId: 'lend-usdc' },
        { id: 'a', villageId: 'lend-usdc' },
        { id: 'c', villageId: null },
      ],
      villages,
    )
    expect(spots.get('a')).toEqual(queueSlot(LEND_USDC, 0))
    expect(spots.get('b')).toEqual(queueSlot(LEND_USDC, 1))
    expect(spots.get('c')).toEqual(arenaSlot(0))
  })

  it('sends a character with an unknown village to the arena', () => {
    const spots = layoutStandings([{ id: 'a', villageId: 'gone' }], villages)
    expect(spots.get('a')).toEqual(arenaSlot(0))
  })

  it('does not depend on the input order', () => {
    const one = layoutStandings(
      [
        { id: 'x', villageId: 'frax' },
        { id: 'y', villageId: 'frax' },
      ],
      villages,
    )
    const two = layoutStandings(
      [
        { id: 'y', villageId: 'frax' },
        { id: 'x', villageId: 'frax' },
      ],
      villages,
    )
    expect(one).toEqual(two)
  })
})

describe('buildTownTiles', () => {
  const tiles = buildTownTiles([pixelToTile(LEND_USDC.x, LEND_USDC.y)])

  it('is a 25 × 25 grid', () => {
    expect(tiles).toHaveLength(MAP_TILES)
    for (const row of tiles) expect(row).toHaveLength(MAP_TILES)
  })

  it('draws a road on the diagonal from the village towards the arena', () => {
    // Village (200, 200) = tile (6, 6); the arena starts at tile 10.
    for (let t = 6; t <= 9; t++) expect(tiles[t][t]).toBe(TILES.road)
  })

  it('keeps the arena border intact (roads stop at its edge)', () => {
    expect(tiles[10][10]).toBe(TILES.arena[0][0])
    expect(tiles[12][12]).toBe(TILES.arena[1][1])
    expect(tiles[14][14]).toBe(TILES.arena[2][2])
  })
})

describe('sprites', () => {
  it('falls back to the default sprite for unknown ids', () => {
    expect(knownSprite('princess')).toBe('princess')
    expect(knownSprite('merchant_1')).toBe('boy')
  })

  it('has 6 to 10 characters to pick from (F3.1)', () => {
    expect(SPRITES.length).toBeGreaterThanOrEqual(6)
    expect(SPRITES.length).toBeLessThanOrEqual(10)
  })

  it('reads one column of the walk sheet per direction', () => {
    expect(walkFrames('down')).toEqual([0, 4, 8, 12])
    expect(walkFrames('right')).toEqual([3, 7, 11, 15])
  })

  it('faces up or down on an exact diagonal', () => {
    expect(directionOf(10, 10)).toBe('down')
    expect(directionOf(-10, -10)).toBe('up')
    expect(directionOf(-20, 5)).toBe('left')
  })
})
