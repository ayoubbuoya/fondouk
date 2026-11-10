import { describe, expect, it } from 'vitest'

import type { AvatarSummary, Village } from './api'
import { townStats } from './townStats'

const village = (over: Partial<Village>): Village => ({
  id: 'v',
  name: 'V',
  kind: 'lending',
  asset: 'USDC',
  map_x: 0,
  map_y: 0,
  apy_base: '10',
  apy_reward: '0',
  tvl_usd: '1000',
  source_ts: null,
  last_fetched_at: null,
  stale: false,
  ...over,
})

const avatar = (over: Partial<AvatarSummary>): AvatarSummary => ({
  id: 'a',
  owner: 'o.testnet',
  name: 'A',
  sprite: 'boy',
  mode: 'sim',
  active: true,
  cloned_from: null,
  listed: false,
  created_at: '2026-11-01T00:00:00Z',
  equity_usd: '100',
  main_village: null,
  deposit_tx_hash: null,
  deposit_token: null,
  ...over,
})

describe('townStats', () => {
  it('is all zeros for an empty town', () => {
    expect(townStats([], [])).toEqual({
      agents: 0,
      bestApy: null,
      poolsTvlUsd: 0,
      simEquityUsd: 0,
      realEquityUsd: 0,
      realAgents: 0,
    })
  })

  it('counts every character, paused ones too', () => {
    const stats = townStats([], [avatar({ id: 'a' }), avatar({ id: 'b', active: false })])
    expect(stats.agents).toBe(2)
  })

  it('takes the best APY from fresh villages only (base + reward)', () => {
    const stats = townStats(
      [
        village({ id: 'fresh', apy_base: '8', apy_reward: '1.5' }),
        village({ id: 'stale', apy_base: '40', stale: true }),
        village({ id: 'no-data', apy_base: null, apy_reward: null }),
      ],
      [],
    )
    expect(stats.bestApy).toBeCloseTo(9.5)
  })

  it('has no best APY when every village is stale', () => {
    expect(townStats([village({ stale: true })], []).bestApy).toBeNull()
  })

  it('adds up the TVL of all villages and ignores missing values', () => {
    const stats = townStats(
      [village({ tvl_usd: '1500.5' }), village({ tvl_usd: null }), village({ tvl_usd: '500', stale: true })],
      [],
    )
    expect(stats.poolsTvlUsd).toBeCloseTo(2000.5)
  })

  it('never adds sim and real equity together', () => {
    const stats = townStats(
      [],
      [
        avatar({ id: 'a', mode: 'sim', equity_usd: '10000' }),
        avatar({ id: 'b', mode: 'sim', equity_usd: '250.5' }),
        avatar({ id: 'c', mode: 'real', equity_usd: '50' }),
      ],
    )
    expect(stats.simEquityUsd).toBeCloseTo(10250.5)
    expect(stats.realEquityUsd).toBeCloseTo(50)
    expect(stats.realAgents).toBe(1)
  })

  it('counts a broken equity value as 0 instead of NaN', () => {
    expect(townStats([], [avatar({ equity_usd: 'oops' })]).simEquityUsd).toBe(0)
  })
})
