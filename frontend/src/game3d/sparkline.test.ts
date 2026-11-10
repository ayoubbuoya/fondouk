import { describe, expect, it } from 'vitest'

import { sparklinePoints } from './sparkline'

describe('sparklinePoints', () => {
  it('spreads the values over the width, highest at the top', () => {
    expect(sparklinePoints([1, 3, 2], 90, 26)).toBe('0,26 45,0 90,13')
  })

  it('draws a flat history through the middle', () => {
    expect(sparklinePoints([4, 4], 90, 26)).toBe('0,13 90,13')
  })

  it('draws nothing for fewer than 2 values', () => {
    expect(sparklinePoints([], 90, 26)).toBe('')
    expect(sparklinePoints([5], 90, 26)).toBe('')
  })
})
