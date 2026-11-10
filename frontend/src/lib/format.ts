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

const YOCTO_PER_NEAR = 10n ** 24n

/**
 * `"100000000000000000000000"` (yoctoNEAR) -> `"0.1 NEAR"`. Up to 4 decimals,
 * cut (not rounded) so the label never shows more than the real price.
 * BigInt keeps all digits: a price does not fit a JS number. Broken input -> `"–"`.
 */
export function formatNear(yocto: string): string {
  if (!/^\d+$/.test(yocto)) return '–'
  const value = BigInt(yocto)
  const whole = value / YOCTO_PER_NEAR
  const fraction = (value % YOCTO_PER_NEAR).toString().padStart(24, '0').slice(0, 4).replace(/0+$/, '')
  return `${whole}${fraction === '' ? '' : `.${fraction}`} NEAR`
}

/**
 * Converts a human-readable NEAR amount (e.g. "0.5" or "1") into yoctoNEAR (1 NEAR = 10^24 yoctoNEAR).
 * Returns null if the input is not a valid non-negative decimal string or has more than 24 decimals.
 */
export function parseNearToYocto(near: string | number): string | null {
  const s = String(near).trim()
  if (!s || !/^\d+(\.\d+)?$/.test(s)) return null
  const [whole, fraction = ''] = s.split('.')
  if (fraction.length > 24) return null
  const paddedFraction = fraction.padEnd(24, '0')
  const yocto = BigInt(whole) * YOCTO_PER_NEAR + BigInt(paddedFraction)
  return yocto.toString()
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

/**
 * Formats a decimal return string or number as a signed percentage.
 * e.g. "0.052" -> "+5.2%", "-0.014" -> "-1.4%", "0" -> "0.0%".
 */
export function formatPercent(value: string | number | null | undefined): string {
  if (value === null || value === undefined || value === '') return '–'
  const num = typeof value === 'string' ? parseFloat(value) : value
  if (!Number.isFinite(num)) return '–'
  const pct = num * 100
  const sign = pct > 0 ? '+' : ''
  return `${sign}${pct.toFixed(1)}%`
}

/**
 * Formats peak-to-trough drawdown as a percentage (e.g. "0.025" -> "-2.5%", "0" -> "0.0%").
 */
export function formatDrawdown(value: string | number | null | undefined): string {
  if (value === null || value === undefined || value === '') return '0.0%'
  const num = typeof value === 'string' ? parseFloat(value) : value
  if (!Number.isFinite(num) || num <= 0) return '0.0%'
  return `-${(num * 100).toFixed(1)}%`
}

/**
 * Formats an avatar's age in days, e.g. 0 -> "<1d", 1 -> "1d", 14 -> "14d".
 */
export function formatAge(days: number): string {
  if (days <= 0) return '<1d'
  return `${days}d`
}

/**
 * Formats an ISO timestamp as a relative time, e.g. "just now", "5m ago", "2h ago", "3d ago".
 */
export function formatTimeAgo(isoString: string, now = new Date()): string {
  const ts = Date.parse(isoString)
  if (Number.isNaN(ts)) return '–'
  const seconds = Math.max(0, Math.floor((now.getTime() - ts) / 1000))
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}

const compactUsd = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  notation: 'compact',
  maximumFractionDigits: 1,
})

/** `3312456` -> `"$3.3M"`, `98420` -> `"$98.4K"`: for the HUD chips, where space is short. */
export function formatCompactUsd(amount: number): string {
  return Number.isFinite(amount) ? compactUsd.format(amount) : '–'
}

/** One slice of a character's money: a village id, or `"cash"`. */
export interface AllocationShare {
  key: string
  /** 0–100, of the character's total (positions + cash). */
  percent: number
}

/**
 * How a character's money is split, for the allocation bar. Villages come
 * first, largest first; cash is always last. Empty slices are left out, and
 * a character with no money at all gives `[]`.
 */
export function allocationShares(positions: Record<string, string>, cashUsd: string): AllocationShare[] {
  const toAmount = (value: string) => {
    const parsed = Number(value)
    return Number.isFinite(parsed) && parsed > 0 ? parsed : 0
  }
  const villages = Object.entries(positions)
    .map(([key, usd]) => ({ key, usd: toAmount(usd) }))
    .sort((a, b) => b.usd - a.usd)
  const slices = [...villages, { key: 'cash', usd: toAmount(cashUsd) }].filter((slice) => slice.usd > 0)
  const total = slices.reduce((sum, slice) => sum + slice.usd, 0)
  return slices.map((slice) => ({ key: slice.key, percent: (slice.usd / total) * 100 }))
}
