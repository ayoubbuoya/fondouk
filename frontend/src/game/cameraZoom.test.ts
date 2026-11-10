import { describe, expect, it } from 'vitest'

import { MAX_ZOOM, nextZoom, ZOOM_STEP, zoomLimits } from './cameraZoom'

const MIN = 0.5

describe('nextZoom', () => {
  it('zooms in one step when the wheel goes up (negative delta)', () => {
    expect(nextZoom(1, -100, MIN)).toBeCloseTo(ZOOM_STEP)
  })

  it('zooms out one step when the wheel goes down (positive delta)', () => {
    expect(nextZoom(2, 100, MIN)).toBeCloseTo(2 / ZOOM_STEP)
  })

  it('keeps the zoom when the delta is 0 (horizontal-only scroll)', () => {
    expect(nextZoom(1.5, 0, MIN)).toBe(1.5)
  })

  it('never zooms out past the minimum', () => {
    expect(nextZoom(MIN, 100, MIN)).toBe(MIN)
    expect(nextZoom(MIN * 1.05, 100, MIN)).toBe(MIN)
  })

  it('never zooms in past the maximum', () => {
    expect(nextZoom(MAX_ZOOM, -100, MIN)).toBe(MAX_ZOOM)
    expect(nextZoom(2.9, -100, MIN)).toBe(MAX_ZOOM)
  })
})

describe('zoomLimits', () => {
  const world = { w: 2432, h: 1472 }
  const town = { w: 1920, h: 1280 }

  it('never lets the view leave the world (no empty bars)', () => {
    const { min } = zoomLimits({ w: 1200, h: 800 }, world, town)
    expect(world.w * min).toBeGreaterThanOrEqual(1200)
    expect(world.h * min).toBeGreaterThanOrEqual(800)
  })

  it('starts with the whole town visible when the view is wide enough', () => {
    const view = { w: 1500, h: 800 }
    const { min, start } = zoomLimits(view, world, town)
    expect(start).toBeGreaterThanOrEqual(min)
    expect(town.w * start).toBeLessThanOrEqual(view.w)
    expect(town.h * start).toBeLessThanOrEqual(view.h)
  })

  it('starts at the minimum when the view is too tall to show the whole town', () => {
    const { min, start } = zoomLimits({ w: 600, h: 1000 }, world, town)
    expect(start).toBe(min)
  })

  it('never goes above the maximum zoom on a huge screen', () => {
    const { min, start } = zoomLimits({ w: 20000, h: 12000 }, world, town)
    expect(min).toBe(MAX_ZOOM)
    expect(start).toBe(MAX_ZOOM)
  })
})
