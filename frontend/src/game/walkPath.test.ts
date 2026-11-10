import { describe, expect, it } from 'vitest'

import {
  assignDistricts,
  COURT,
  DISTRICTS,
  MAIN_STREET_Y,
  solidRects,
  SPARE_PLOTS,
  toPixels,
  type Point,
} from './medinaLayout'
import { directionOf, knownSprite, SPRITES, walkFrames } from './sprites'
import {
  courtSlot,
  districtSlot,
  layoutStandings,
  layoutTagTiers,
  pathLength,
  placeOf,
  walkPath,
  walkSpeed,
} from './walkPath'

const LEND_USDC = DISTRICTS['rhea-lend-usdc']
const FRAX = DISTRICTS['rhea-dex-frax-usdc']
const districts = assignDistricts(Object.keys(DISTRICTS))

const key = (point: Point) => `${Math.round(point.x)},${Math.round(point.y)}`

const insideSolid = (point: Point) =>
  solidRects().some((solid) => {
    const box = toPixels(solid)
    return point.x > box.x && point.x < box.x + box.w && point.y > box.y && point.y < box.y + box.h
  })

describe('walkPath', () => {
  const from = districtSlot(LEND_USDC, 0)

  it('is empty when the character is already there', () => {
    expect(walkPath(from, from, LEND_USDC, LEND_USDC)).toEqual([])
  })

  it('takes one straight step when only the spot changes', () => {
    const next = districtSlot(LEND_USDC, 1)
    expect(walkPath(districtSlot(LEND_USDC, 4), next, LEND_USDC, LEND_USDC)).toEqual([next])
  })

  it('goes down its street, along the main street and up the other street', () => {
    const to = districtSlot(FRAX, 0)
    const path = walkPath(from, to, LEND_USDC, FRAX)
    expect(path).toEqual([...LEND_USDC.street, ...[...FRAX.street].reverse(), to])
    // Both junctions are on the main street, so the middle step is along it.
    expect(path[LEND_USDC.street.length - 1].y).toBe(MAIN_STREET_Y)
    expect(path[LEND_USDC.street.length].y).toBe(MAIN_STREET_Y)
  })

  it('reaches the court (cash) through the hub', () => {
    const to = courtSlot(0)
    expect(walkPath(from, to, LEND_USDC, COURT)).toEqual([...LEND_USDC.street, ...COURT.street, to])
  })

  it('has no zero-length step', () => {
    // Leaving from exactly the first street point must not repeat it.
    const path = walkPath(LEND_USDC.street[0], courtSlot(0), LEND_USDC, COURT)
    expect(path[0]).not.toEqual(LEND_USDC.street[0])
  })

  it('never takes longer than 5 seconds, even across the whole town (F8)', () => {
    const places = [...Object.values(DISTRICTS), ...SPARE_PLOTS]
    for (const a of places) {
      for (const b of places) {
        if (a === b) continue
        const start = districtSlot(a, 11)
        const length = pathLength(start, walkPath(start, districtSlot(b, 11), a, b))
        expect(length / walkSpeed(length)).toBeLessThanOrEqual(5)
      }
    }
  })

  it('keeps the normal speed on short walks', () => {
    expect(walkSpeed(100)).toBe(200)
  })
})

describe('districtSlot and courtSlot', () => {
  it('gives a different spot to each of the first 12 at a district', () => {
    const spots = new Set(Array.from({ length: 12 }, (_, i) => key(districtSlot(LEND_USDC, i))))
    expect(spots.size).toBe(12)
  })

  it('gives a different spot to each of the first 16 cash characters', () => {
    const spots = new Set(Array.from({ length: 16 }, (_, i) => key(courtSlot(i))))
    expect(spots.size).toBe(16)
  })

  it('never puts a character on a building or a statue', () => {
    for (const district of [...Object.values(DISTRICTS), ...SPARE_PLOTS]) {
      for (let i = 0; i < 12; i++) expect(insideSolid(districtSlot(district, i))).toBe(false)
    }
    for (let i = 0; i < 16; i++) expect(insideSolid(courtSlot(i))).toBe(false)
  })

  it('stands below the door, in front of the building', () => {
    for (let i = 0; i < 12; i++) {
      expect(districtSlot(LEND_USDC, i).y).toBeGreaterThan(LEND_USDC.slotsOrigin.y)
    }
  })
})

describe('layoutStandings', () => {
  it('puts characters at their village, sorted by id, and cash in the court', () => {
    const spots = layoutStandings(
      [
        { id: 'b', villageId: 'rhea-lend-usdc' },
        { id: 'a', villageId: 'rhea-lend-usdc' },
        { id: 'c', villageId: null },
      ],
      districts,
    )
    expect(spots.get('a')).toEqual(districtSlot(LEND_USDC, 0))
    expect(spots.get('b')).toEqual(districtSlot(LEND_USDC, 1))
    expect(spots.get('c')).toEqual(courtSlot(0))
  })

  it('sends a character with an unknown village to the court', () => {
    const spots = layoutStandings([{ id: 'a', villageId: 'gone' }], districts)
    expect(spots.get('a')).toEqual(courtSlot(0))
    expect(placeOf('gone', districts)).toBe(COURT)
    expect(placeOf(null, districts)).toBe(COURT)
  })

  it('does not depend on the input order', () => {
    const one = layoutStandings(
      [
        { id: 'x', villageId: 'rhea-dex-frax-usdc' },
        { id: 'y', villageId: 'rhea-dex-frax-usdc' },
      ],
      districts,
    )
    const two = layoutStandings(
      [
        { id: 'y', villageId: 'rhea-dex-frax-usdc' },
        { id: 'x', villageId: 'rhea-dex-frax-usdc' },
      ],
      districts,
    )
    expect(one).toEqual(two)
  })
})

describe('layoutTagTiers', () => {
  const at = (villageId: string | null, ...ids: string[]) => ids.map((id) => ({ id, villageId }))

  it('keeps a lone character at tier 0', () => {
    expect(layoutTagTiers(at('rhea-lend-usdc', 'a'), districts).get('a')).toBe(0)
  })

  it('makes a staircase of one row, in the same id order as the spots', () => {
    const standings = at('rhea-lend-usdc', 'd', 'b', 'a', 'c')

    const tiers = layoutTagTiers(standings, districts)

    expect([...tiers].sort()).toEqual([
      ['a', 0],
      ['b', 1],
      ['c', 2],
      ['d', 3],
    ])
    // Neighbours in the row never share a height: that is what stops the overlap.
    const spots = layoutStandings(standings, districts)
    expect(spots.get('a')!.y).toBe(spots.get('d')!.y)
  })

  it('starts the second row again at tier 0', () => {
    const tiers = layoutTagTiers(at('rhea-lend-usdc', 'a', 'b', 'c', 'd', 'e'), districts)

    expect(tiers.get('e')).toBe(0)
  })

  it('tiers each place on its own, the court included', () => {
    const tiers = layoutTagTiers(
      [
        ...at('rhea-lend-usdc', 'a', 'b'),
        ...at('rhea-dex-frax-usdc', 'c'),
        ...at(null, 'd', 'e'),
        ...at('gone', 'f'), // unknown village: stands in the court
      ],
      districts,
    )

    expect(tiers.get('b')).toBe(1)
    expect(tiers.get('c')).toBe(0)
    expect(tiers.get('d')).toBe(0)
    expect(tiers.get('e')).toBe(1)
    expect(tiers.get('f')).toBe(2)
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
