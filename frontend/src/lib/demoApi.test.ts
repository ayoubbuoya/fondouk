import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  resetRealRates,
  triggerDemoStep,
  triggerMarketShock,
  triggerTick,
} from './demoApi'

describe('demoApi', () => {
  const originalFetch = globalThis.fetch

  beforeEach(() => {
    globalThis.fetch = vi.fn()
  })

  afterEach(() => {
    globalThis.fetch = originalFetch
  })

  it('triggerMarketShock posts preset and returns result', async () => {
    const mockResponse = { status: 'ok', village_id: 'rhea-lend-usdt', apy: '22.5' }
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => mockResponse,
    } as Response)

    const res = await triggerMarketShock('usdt_spike')
    expect(res).toEqual(mockResponse)
    expect(globalThis.fetch).toHaveBeenCalledWith('/api/demo/shock', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ preset: 'usdt_spike' }),
    })
  })

  it('triggerTick posts with relaxArena and bypassCooldown flags', async () => {
    const mockResponse = { status: 'ok', decisions: 10 }
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => mockResponse,
    } as Response)

    const res = await triggerTick(true, true)
    expect(res).toEqual(mockResponse)
    expect(globalThis.fetch).toHaveBeenCalledWith('/api/demo/tick', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ relax_arena_rate_limit: true, bypass_cooldown: true }),
    })
  })

  it('triggerDemoStep posts preset to /demo/step', async () => {
    const mockResponse = {
      status: 'ok',
      shock: { status: 'ok', village_id: 'rhea-lend-usdt', apy: '22.5' },
      tick: { status: 'ok', decisions: 10 },
    }
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => mockResponse,
    } as Response)

    const res = await triggerDemoStep('usdt_spike')
    expect(res).toEqual(mockResponse)
    expect(globalThis.fetch).toHaveBeenCalledWith('/api/demo/step', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ preset: 'usdt_spike' }),
    })
  })

  it('resetRealRates posts to /demo/reset', async () => {
    const mockResponse = { status: 'ok', message: 'reset to baseline APYs' }
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => mockResponse,
    } as Response)

    const res = await resetRealRates()
    expect(res).toEqual(mockResponse)
    expect(globalThis.fetch).toHaveBeenCalledWith('/api/demo/reset', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({}),
    })
  })
})
