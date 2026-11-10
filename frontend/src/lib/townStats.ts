/**
 * The numbers in the top HUD, computed from data the town already loaded
 * (`GET /villages`, `GET /avatars`). Pure, no DOM: unit-tested.
 *
 * Numbers (not decimals) are fine here: this is for display only, never for money.
 */

import type { AvatarSummary, Village } from './api'
import { totalApy } from './format'

export interface TownStats {
  /** Characters on the map, running or paused. */
  agents: number
  /** Highest total APY among villages that are not stale; `null` when there is none. */
  bestApy: number | null
  /** Sum of the villages' TVL (DefiLlama), stale ones included: it is their last known size. */
  poolsTvlUsd: number
  /**
   * Paper money and vault money are **two numbers, never added** (CLAUDE.md
   * "data source labels"): one total would mix simulated and real results.
   */
  simEquityUsd: number
  realEquityUsd: number
  /** The real chip is hidden when nobody runs in real mode. */
  realAgents: number
}

/** A decimal string as a number; missing or broken values count as 0. */
function amount(value: string | null | undefined): number {
  const parsed = Number(value ?? '0')
  return Number.isFinite(parsed) ? parsed : 0
}

export function townStats(villages: Village[], avatars: AvatarSummary[]): TownStats {
  let bestApy: number | null = null
  let poolsTvlUsd = 0
  for (const village of villages) {
    poolsTvlUsd += amount(village.tvl_usd)
    const apy = village.stale ? null : totalApy(village)
    if (apy !== null && (bestApy === null || apy > bestApy)) bestApy = apy
  }

  let simEquityUsd = 0
  let realEquityUsd = 0
  let realAgents = 0
  for (const avatar of avatars) {
    if (avatar.mode === 'real') {
      realEquityUsd += amount(avatar.equity_usd)
      realAgents += 1
    } else {
      simEquityUsd += amount(avatar.equity_usd)
    }
  }

  return { agents: avatars.length, bestApy, poolsTvlUsd, simEquityUsd, realEquityUsd, realAgents }
}
