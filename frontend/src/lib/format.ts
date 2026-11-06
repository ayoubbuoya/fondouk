/**
 * Pure display helpers for villages and characters. No DOM, no Phaser: unit-tested.
 */

import type { AgentAction, Village } from './api'

/**
 * Base + reward APY as a number, or `null` when the village has no data yet.
 * Numbers are fine here: this is for display and drawing only, never for money.
 */
export function totalApy(village: Pick<Village, 'apy_base' | 'apy_reward'>): number | null {
  if (village.apy_base === null) return null
  return Number(village.apy_base) + Number(village.apy_reward ?? '0')
}

/** `14.01014` -> `"14.0%"`; `null` -> `"–"` (no data yet). */
export function formatApy(apy: number | null): string {
  return apy === null ? '–' : `${apy.toFixed(1)}%`
}

/**
 * F2.4: a stale village shows the age of its last value, e.g.
 * `"stale · 3 h ago"`. Returns `null` for a fresh village (nothing to show).
 * `lastFetchedAt = null` means the indexer never fetched it.
 */
export function staleLabel(
  village: Pick<Village, 'stale' | 'last_fetched_at'>,
  now: Date,
): string | null {
  if (!village.stale) return null
  if (village.last_fetched_at === null) return 'stale · no data yet'
  const minutes = Math.max(0, Math.floor((now.getTime() - Date.parse(village.last_fetched_at)) / 60_000))
  if (minutes < 60) return `stale · ${minutes} min ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 48) return `stale · ${hours} h ago`
  return `stale · ${Math.floor(hours / 24)} d ago`
}

const usd = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

/** `"10234.5678"` -> `"$10,234.57"`. A missing or broken value shows `"–"`. */
export function formatUsd(value: string | null | undefined): string {
  if (value === null || value === undefined || value.trim() === '') return '–'
  const amount = Number(value)
  return Number.isFinite(amount) ? usd.format(amount) : '–'
}

/**
 * A short, public label for one applied action (SPEC §5), e.g.
 * `"MOVE 40% rhea-lend-usdc → rhea-dex-stable4"`. The action's own `reason`
 * field is left out on purpose: the panel shows `reason_public` instead.
 */
export function actionLabel(action: AgentAction): string {
  switch (action.action) {
    case 'HOLD':
      return 'HOLD'
    case 'MOVE':
      return `MOVE ${action.percent}% ${action.from} → ${action.to}`
    case 'REBALANCE': {
      // Sorted by id so the same weights always read the same way.
      const parts = Object.entries(action.weights)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([village, percent]) => `${village} ${percent}%`)
      return parts.length === 0 ? 'REBALANCE (all cash)' : `REBALANCE ${parts.join(', ')}`
    }
    case 'SAY':
      return 'SAY'
    default:
      // A newer backend may add actions; show the raw name, never crash.
      return String((action as { action?: unknown }).action ?? '?')
  }
}
