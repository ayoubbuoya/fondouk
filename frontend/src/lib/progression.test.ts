import { describe, expect, it } from 'vitest'

import { badgeIcon, celebrationText, nameTag, xpPercent } from './progression'

describe('nameTag', () => {
  it('puts the level as a short number after the name', () => {
    expect(nameTag('Amira', 4)).toBe('Amira 4')
  })
})

describe('xpPercent', () => {
  it('is the share of the current level that is done', () => {
    expect(xpPercent(0, 100)).toBe(0)
    expect(xpPercent(50, 200)).toBe(25)
    expect(xpPercent(200, 200)).toBe(100)
  })

  it('stays between 0 and 100 and survives a missing span', () => {
    expect(xpPercent(300, 200)).toBe(100)
    expect(xpPercent(-5, 200)).toBe(0)
    expect(xpPercent(10, 0)).toBe(0)
    expect(xpPercent(10, Number.NaN)).toBe(0)
  })
})

describe('celebrationText', () => {
  it('shouts for a level and names a badge', () => {
    expect(celebrationText('level', 'Level 4')).toBe('LEVEL UP!')
    expect(celebrationText('badge', 'First Steps')).toBe('🏅 First Steps')
  })
})

describe('badgeIcon', () => {
  it('has its own icon for a known badge and a default for an unknown one', () => {
    expect(badgeIcon('first_steps')).toBe('👣')
    expect(badgeIcon('added_by_a_newer_backend')).toBe('🏅')
  })
})
