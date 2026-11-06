/**
 * Pure display helpers for village data. No DOM, no Phaser: unit-tested.
 */

import type { Village } from './api'

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
