import { describe, expect, it } from 'vitest'

import {
  assignDistricts,
  COURT,
  DECOR_BUILDINGS,
  DECOR_PROPS,
  DISTRICTS,
  districtFrame,
  FRAMES,
  GATE,
  HUB,
  LANDMARKS,
  MAIN_STREET_Y,
  PAVED,
  solidRects,
  SPARE_PLOTS,
  TILE_PX,
  toPixels,
  TOWN,
  WALL,
  wallProps,
  WORLD_TILES,
  type TileRect,
} from './medinaLayout'

/** The 4 village ids seeded by backend migration 0002. */
const SEEDED_VILLAGES = ['rhea-lend-usdc', 'rhea-lend-usdt', 'rhea-dex-stable4', 'rhea-dex-frax-usdc']

const overlaps = (a: TileRect, b: TileRect) =>
  a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h

const inside = (inner: TileRect, outer: TileRect) =>
  inner.x >= outer.x &&
  inner.y >= outer.y &&
  inner.x + inner.w <= outer.x + outer.w &&
  inner.y + inner.h <= outer.y + outer.h

const allDistricts = [...Object.values(DISTRICTS), ...SPARE_PLOTS]

describe('districts', () => {
  it('has a district for every seeded village', () => {
    for (const id of SEEDED_VILLAGES) expect(DISTRICTS[id]).toBeDefined()
  })

  it('keeps every district building inside the wall', () => {
    for (const district of allDistricts) expect(inside(district.building, WALL)).toBe(true)
  })

  it('ends every street on the main street', () => {
    for (const place of [...allDistricts, COURT]) {
      expect(place.street.length).toBeGreaterThan(0)
      expect(place.street.at(-1)?.y).toBe(MAIN_STREET_Y)
    }
    expect(COURT.street).toEqual([HUB])
  })

  it('puts the door of each building at the bottom centre', () => {
    for (const district of allDistricts) {
      const box = toPixels(district.building)
      expect(district.slotsOrigin).toEqual({ x: box.x + box.w / 2, y: box.y + box.h })
    }
  })
})

describe('assignDistricts', () => {
  it('gives known villages their own district', () => {
    const assigned = assignDistricts(SEEDED_VILLAGES)
    for (const id of SEEDED_VILLAGES) expect(assigned.get(id)).toBe(DISTRICTS[id])
  })

  it('gives unknown villages the spare plots, in id order', () => {
    const assigned = assignDistricts(['zeta', 'rhea-lend-usdc', 'alpha'])
    expect(assigned.get('alpha')).toBe(SPARE_PLOTS[0])
    expect(assigned.get('zeta')).toBe(SPARE_PLOTS[1])
  })

  it('leaves out villages when the spare plots run out', () => {
    const assigned = assignDistricts(['a', 'b', 'c'])
    expect(assigned.has('a')).toBe(true)
    expect(assigned.has('b')).toBe(true)
    expect(assigned.has('c')).toBe(false)
  })
})

describe('buildings', () => {
  it('never puts two solid things on the same tiles', () => {
    const solids = solidRects()
    for (let i = 0; i < solids.length; i++) {
      for (let j = i + 1; j < solids.length; j++) {
        expect(overlaps(solids[i], solids[j]), `solid ${i} overlaps solid ${j}`).toBe(false)
      }
    }
  })

  it('keeps landmarks and decor buildings inside the wall', () => {
    for (const building of [...LANDMARKS.map((l) => l.prop), ...DECOR_BUILDINGS]) {
      expect(inside(building.tiles, WALL), building.frame).toBe(true)
    }
  })

  it('keeps every prop inside the world', () => {
    const world = { x: 0, y: 0, w: WORLD_TILES.w, h: WORLD_TILES.h }
    for (const item of [...DECOR_PROPS, ...DECOR_BUILDINGS, ...wallProps(), GATE]) {
      expect(inside(item.tiles, world), item.frame).toBe(true)
    }
  })

  it('sizes each prop from its frame', () => {
    for (const item of [...DECOR_PROPS, ...DECOR_BUILDINGS]) {
      const [, , w, h] = FRAMES[item.frame]
      expect([item.tiles.w, item.tiles.h]).toEqual([w, h])
    }
  })

  it('draws lending villages and pools with different houses', () => {
    expect(districtFrame('lending')).not.toBe(districtFrame('stable_lp'))
  })
})

describe('ground and wall', () => {
  it('has the wall inside the town and the town inside the world', () => {
    expect(inside(WALL, TOWN)).toBe(true)
    expect(inside(TOWN, { x: 0, y: 0, w: WORLD_TILES.w, h: WORLD_TILES.h })).toBe(true)
  })

  it('paves only areas at least 2 tiles wide and high (no border tiles for strips)', () => {
    for (const area of PAVED) {
      expect(area.w).toBeGreaterThanOrEqual(2)
      expect(area.h).toBeGreaterThanOrEqual(2)
    }
  })

  it('paves the hub and the end of every street', () => {
    const paved = (x: number, y: number) =>
      PAVED.some(
        (area) =>
          x >= area.x * TILE_PX &&
          x <= (area.x + area.w) * TILE_PX &&
          y >= area.y * TILE_PX &&
          y <= (area.y + area.h) * TILE_PX,
      )
    expect(paved(HUB.x, HUB.y)).toBe(true)
    for (const district of allDistricts) {
      for (const point of district.street) expect(paved(point.x, point.y)).toBe(true)
    }
  })

  it('leaves a gap in the south wall for the gate', () => {
    const gate = toPixels(GATE.tiles)
    const south = (WALL.y + WALL.h - 1) * TILE_PX
    const blocking = wallProps().filter((piece) => {
      const box = toPixels(piece.tiles)
      return piece.frame === 'wallH' && box.y === south && box.x >= gate.x && box.x + box.w <= gate.x + gate.w
    })
    expect(blocking).toEqual([])
  })
})
