import { describe, expect, it } from 'vitest'

import { isTownView3d } from './townView'

describe('isTownView3d', () => {
  it('is on for ?view=3d, alone or with other parameters', () => {
    expect(isTownView3d('?view=3d')).toBe(true)
    expect(isTownView3d('?tour=1&view=3d')).toBe(true)
  })

  it('is off without the parameter or with another value', () => {
    expect(isTownView3d('')).toBe(false)
    expect(isTownView3d('?view=2d')).toBe(false)
    expect(isTownView3d('?view=3D')).toBe(false)
  })

  it('is off outside a browser (no window)', () => {
    expect(isTownView3d()).toBe(false)
  })
})
