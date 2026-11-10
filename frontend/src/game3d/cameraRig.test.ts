import { describe, expect, it } from 'vitest'

import { clampToRect, distanceLimits, easeInOut, fitDistance, MIN_DISTANCE, START_TILT } from './cameraRig'

const TOWN = { w: 60, d: 40 }
const NO_BARS = { top: 0, bottom: 0 }

describe('fitDistance', () => {
  it('moves back when the window gets narrower', () => {
    const wide = fitDistance(TOWN, { w: 1600, h: 900 }, NO_BARS, START_TILT)
    const narrow = fitDistance(TOWN, { w: 800, h: 900 }, NO_BARS, START_TILT)
    expect(narrow).toBeGreaterThan(wide)
  })

  it('moves back when the HUD bars cover part of a wide, short view', () => {
    const view = { w: 2400, h: 500 }
    const without = fitDistance(TOWN, view, NO_BARS, START_TILT)
    const withBars = fitDistance(TOWN, view, { top: 60, bottom: 36 }, START_TILT)
    expect(withBars).toBeGreaterThan(without)
  })

  it('needs less distance for the depth when tilted more (the depth looks shorter)', () => {
    const view = { w: 3000, h: 600 } // depth is the tight side
    expect(fitDistance(TOWN, view, NO_BARS, 1.1)).toBeLessThan(fitDistance(TOWN, view, NO_BARS, 0.4))
  })

  it('never returns less than the minimum, even for a view that is not laid out yet', () => {
    expect(fitDistance(TOWN, { w: 0, h: 0 }, NO_BARS, START_TILT)).toBe(MIN_DISTANCE)
    expect(fitDistance({ w: 1, d: 1 }, { w: 1000, h: 1000 }, NO_BARS, START_TILT)).toBe(MIN_DISTANCE)
  })
})

describe('distanceLimits', () => {
  it('lets the user zoom in to the minimum and out a little past the start', () => {
    const limits = distanceLimits(60)
    expect(limits.min).toBe(MIN_DISTANCE)
    expect(limits.max).toBeGreaterThan(60)
  })

  it('keeps min below max when the start is already very close', () => {
    const limits = distanceLimits(MIN_DISTANCE / 2)
    expect(limits.min).toBeLessThanOrEqual(limits.max)
  })
})

describe('clampToRect', () => {
  const rect = { x: 10, z: 5, w: 60, d: 40 }

  it('keeps a point that is inside', () => {
    expect(clampToRect(20, 20, rect)).toEqual({ x: 20, z: 20 })
  })

  it('pulls a point outside back to the edge', () => {
    expect(clampToRect(0, 100, rect)).toEqual({ x: 10, z: 45 })
    expect(clampToRect(999, -3, rect)).toEqual({ x: 70, z: 5 })
  })
})

describe('easeInOut', () => {
  it('goes from 0 to 1 through the middle', () => {
    expect(easeInOut(0)).toBe(0)
    expect(easeInOut(0.5)).toBeCloseTo(0.5)
    expect(easeInOut(1)).toBe(1)
  })

  it('stays inside 0..1 for times outside the move', () => {
    expect(easeInOut(-1)).toBe(0)
    expect(easeInOut(2)).toBe(1)
  })
})
