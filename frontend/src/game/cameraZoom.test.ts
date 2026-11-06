import { describe, expect, it } from 'vitest'

import { MAX_ZOOM, MIN_ZOOM, nextZoom, ZOOM_STEP } from './cameraZoom'

describe('nextZoom', () => {
  it('zooms in one step when the wheel goes up (negative delta)', () => {
    expect(nextZoom(1, -100)).toBeCloseTo(ZOOM_STEP)
  })

  it('zooms out one step when the wheel goes down (positive delta)', () => {
    expect(nextZoom(2, 100)).toBeCloseTo(2 / ZOOM_STEP)
  })

  it('keeps the zoom when the delta is 0 (horizontal-only scroll)', () => {
    expect(nextZoom(1.5, 0)).toBe(1.5)
  })

  it('never zooms out past the whole town', () => {
    expect(nextZoom(MIN_ZOOM, 100)).toBe(MIN_ZOOM)
    expect(nextZoom(1.05, 100)).toBe(MIN_ZOOM)
  })

  it('never zooms in past the maximum', () => {
    expect(nextZoom(MAX_ZOOM, -100)).toBe(MAX_ZOOM)
    expect(nextZoom(2.9, -100)).toBe(MAX_ZOOM)
  })
})
