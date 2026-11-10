import { describe, expect, it } from 'vitest'

import {
  actionLabel,
  allocationShares,
  formatAge,
  formatApy,
  formatCompactUsd,
  formatDrawdown,
  formatNear,
  parseNearToYocto,
  formatPercent,
  formatTimeAgo,
  formatUsd,
  staleLabel,
  totalApy,
} from './format'

describe('allocationShares', () => {
  it('lists villages largest first, then cash, and sums to 100', () => {
    const shares = allocationShares({ small: '100', big: '600' }, '300')
    expect(shares.map((share) => share.key)).toEqual(['big', 'small', 'cash'])
    expect(shares.map((share) => share.percent)).toEqual([60, 10, 30])
    expect(shares.reduce((sum, share) => sum + share.percent, 0)).toBeCloseTo(100)
  })

  it('is one full cash slice for an all-cash character', () => {
    expect(allocationShares({}, '10000')).toEqual([{ key: 'cash', percent: 100 }])
  })

  it('leaves out empty slices', () => {
    expect(allocationShares({ a: '50', b: '0' }, '0')).toEqual([{ key: 'a', percent: 100 }])
  })

  it('is empty when the character has no money or the values are broken', () => {
    expect(allocationShares({}, '0')).toEqual([])
    expect(allocationShares({ a: 'oops' }, '')).toEqual([])
  })
})

describe('formatCompactUsd', () => {
  it('shortens large amounts', () => {
    expect(formatCompactUsd(3_312_456)).toBe('$3.3M')
    expect(formatCompactUsd(98_420)).toBe('$98.4K')
    expect(formatCompactUsd(0)).toBe('$0')
  })

  it('shows a dash for a broken number', () => {
    expect(formatCompactUsd(Number.NaN)).toBe('–')
  })
})

describe('totalApy', () => {
  it('adds base and reward APY from decimal strings', () => {
    expect(totalApy({ apy_base: '14.01014', apy_reward: '0.5' })).toBeCloseTo(14.51014)
  })

  it('treats a missing reward as zero', () => {
    expect(totalApy({ apy_base: '8', apy_reward: null })).toBe(8)
  })

  it('is null when the village has no data yet', () => {
    expect(totalApy({ apy_base: null, apy_reward: null })).toBeNull()
  })
})

describe('formatApy', () => {
  it('rounds to one decimal with a percent sign', () => {
    expect(formatApy(14.01014)).toBe('14.0%')
    expect(formatApy(8.06)).toBe('8.1%')
  })

  it('shows a dash for no data', () => {
    expect(formatApy(null)).toBe('–')
  })
})

describe('staleLabel', () => {
  const now = new Date('2026-11-06T12:00:00Z')

  it('is null for a fresh village', () => {
    expect(staleLabel({ stale: false, last_fetched_at: '2026-11-06T11:30:00Z' }, now)).toBeNull()
  })

  it('shows minutes, hours, then days since the last fetch', () => {
    expect(staleLabel({ stale: true, last_fetched_at: '2026-11-06T11:15:00Z' }, now)).toBe(
      'stale · 45 min ago',
    )
    expect(staleLabel({ stale: true, last_fetched_at: '2026-11-06T09:00:00Z' }, now)).toBe(
      'stale · 3 h ago',
    )
    expect(staleLabel({ stale: true, last_fetched_at: '2026-11-02T12:00:00Z' }, now)).toBe(
      'stale · 4 d ago',
    )
  })

  it('says so when the village was never fetched', () => {
    expect(staleLabel({ stale: true, last_fetched_at: null }, now)).toBe('stale · no data yet')
  })
})

describe('formatUsd', () => {
  it('formats a decimal string as dollars with cents', () => {
    expect(formatUsd('10234.5678')).toBe('$10,234.57')
    expect(formatUsd('0')).toBe('$0.00')
  })

  it('shows a dash for missing or broken values', () => {
    expect(formatUsd(null)).toBe('–')
    expect(formatUsd(undefined)).toBe('–')
    expect(formatUsd('')).toBe('–')
    expect(formatUsd('abc')).toBe('–')
  })
})

describe('formatNear', () => {
  it('converts yoctoNEAR to NEAR without float rounding', () => {
    expect(formatNear('100000000000000000000000')).toBe('0.1 NEAR')
    expect(formatNear('1000000000000000000000000')).toBe('1 NEAR')
    expect(formatNear('2500000000000000000000000')).toBe('2.5 NEAR')
    expect(formatNear('123456789000000000000000000')).toBe('123.4567 NEAR')
  })

  it('cuts tiny prices instead of rounding them up', () => {
    expect(formatNear('1')).toBe('0 NEAR')
    expect(formatNear('99999999999999999999999')).toBe('0.0999 NEAR')
  })

  it('shows a dash for anything that is not a whole number', () => {
    expect(formatNear('')).toBe('–')
    expect(formatNear('1.5')).toBe('–')
    expect(formatNear('-3')).toBe('–')
  })
})

describe('parseNearToYocto', () => {
  it('converts whole and decimal NEAR to yoctoNEAR', () => {
    expect(parseNearToYocto('1')).toBe('1000000000000000000000000')
    expect(parseNearToYocto('0.5')).toBe('500000000000000000000000')
    expect(parseNearToYocto('0.1')).toBe('100000000000000000000000')
    expect(parseNearToYocto('2.5')).toBe('2500000000000000000000000')
    expect(parseNearToYocto(0.5)).toBe('500000000000000000000000')
  })

  it('handles 0', () => {
    expect(parseNearToYocto('0')).toBe('0')
    expect(parseNearToYocto('0.0')).toBe('0')
  })

  it('returns null on invalid input', () => {
    expect(parseNearToYocto('')).toBeNull()
    expect(parseNearToYocto('abc')).toBeNull()
    expect(parseNearToYocto('-1')).toBeNull()
    expect(parseNearToYocto('1.2.3')).toBeNull()
  })
})

describe('actionLabel', () => {
  it('labels HOLD and SAY by name only', () => {
    expect(actionLabel({ action: 'HOLD' })).toBe('HOLD')
    expect(actionLabel({ action: 'SAY', text: 'hello' })).toBe('SAY')
  })

  it('shows percent, source and target of a MOVE', () => {
    expect(actionLabel({ action: 'MOVE', from: 'lend-usdc', to: 'stable4', percent: 40 })).toBe(
      'MOVE 40% lend-usdc → stable4',
    )
  })

  it('lists REBALANCE weights sorted by village id', () => {
    expect(actionLabel({ action: 'REBALANCE', weights: { 'b-pool': 30, 'a-lend': 70 } })).toBe(
      'REBALANCE a-lend 70%, b-pool 30%',
    )
    expect(actionLabel({ action: 'REBALANCE', weights: {} })).toBe('REBALANCE (all cash)')
  })

  it('does not crash on an unknown action', () => {
    expect(actionLabel({ action: 'FLY' } as never)).toBe('FLY')
  })
})

describe('formatPercent', () => {
  it('formats positive percentages with a plus sign', () => {
    expect(formatPercent('0.052')).toBe('+5.2%')
    expect(formatPercent(0.1)).toBe('+10.0%')
  })

  it('formats negative percentages with a minus sign', () => {
    expect(formatPercent('-0.025')).toBe('-2.5%')
    expect(formatPercent(-0.01)).toBe('-1.0%')
  })

  it('formats zero as 0.0%', () => {
    expect(formatPercent('0')).toBe('0.0%')
    expect(formatPercent(0)).toBe('0.0%')
  })

  it('shows a dash for null, undefined, or invalid inputs', () => {
    expect(formatPercent(null)).toBe('–')
    expect(formatPercent(undefined)).toBe('–')
    expect(formatPercent('')).toBe('–')
    expect(formatPercent('not-a-number')).toBe('–')
  })
})

describe('formatDrawdown', () => {
  it('formats positive drop values with a leading minus sign', () => {
    expect(formatDrawdown('0.25')).toBe('-25.0%')
    expect(formatDrawdown(0.015)).toBe('-1.5%')
  })

  it('formats zero or negative values as 0.0%', () => {
    expect(formatDrawdown('0')).toBe('0.0%')
    expect(formatDrawdown(0)).toBe('0.0%')
    expect(formatDrawdown('-0.1')).toBe('0.0%')
  })

  it('handles null, undefined, and empty string as 0.0%', () => {
    expect(formatDrawdown(null)).toBe('0.0%')
    expect(formatDrawdown(undefined)).toBe('0.0%')
    expect(formatDrawdown('')).toBe('0.0%')
  })
})

describe('formatAge', () => {
  it('formats 0 or negative days as <1d', () => {
    expect(formatAge(0)).toBe('<1d')
    expect(formatAge(-1)).toBe('<1d')
  })

  it('formats positive days with a d suffix', () => {
    expect(formatAge(1)).toBe('1d')
    expect(formatAge(14)).toBe('14d')
  })
})

describe('formatTimeAgo', () => {
  const base = new Date('2026-11-10T12:00:00Z')

  it('formats seconds ago as just now', () => {
    expect(formatTimeAgo('2026-11-10T11:59:45Z', base)).toBe('just now')
  })

  it('formats minutes ago', () => {
    expect(formatTimeAgo('2026-11-10T11:45:00Z', base)).toBe('15m ago')
  })

  it('formats hours ago', () => {
    expect(formatTimeAgo('2026-11-10T09:00:00Z', base)).toBe('3h ago')
  })

  it('formats days ago', () => {
    expect(formatTimeAgo('2026-11-08T12:00:00Z', base)).toBe('2d ago')
  })

  it('handles invalid timestamp gracefully', () => {
    expect(formatTimeAgo('invalid', base)).toBe('–')
  })
})
