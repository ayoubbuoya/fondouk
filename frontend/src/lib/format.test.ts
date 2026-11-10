import { describe, expect, it } from 'vitest'

import {
  actionLabel,
  formatAge,
  formatApy,
  formatDrawdown,
  formatNear,
  formatPercent,
  formatUsd,
  staleLabel,
  totalApy,
} from './format'

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
