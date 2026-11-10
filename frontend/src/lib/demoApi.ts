import { ApiError } from './api'

export interface ShockResult {
  status: string
  village_id: string
  apy: string
}

export interface TickResult {
  status: string
  decisions: number
}

export interface StepResult {
  status: string
  shock: ShockResult
  tick: TickResult
}

export interface ResetResult {
  status: string
  message: string
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) {
    const errorText = await res.text().catch(() => '')
    throw new ApiError(res.status, errorText || `POST ${path} failed`)
  }
  return res.json() as Promise<T>
}

/** Injects a yield shock into a village (e.g. USDT spike to 22.5%). */
export async function triggerMarketShock(preset: 'usdt_spike' | 'dex_spike' | 'usdc_drop' = 'usdt_spike'): Promise<ShockResult> {
  return postJson<ShockResult>('/demo/shock', { preset })
}

/** Immediately triggers one agent decision loop across all active characters. */
export async function triggerTick(relaxArenaRateLimit = true, bypassCooldown = true): Promise<TickResult> {
  return postJson<TickResult>('/demo/tick', {
    relax_arena_rate_limit: relaxArenaRateLimit,
    bypass_cooldown: bypassCooldown,
  })
}

/** Combined single-click market shock and immediate tick. */
export async function triggerDemoStep(preset = 'usdt_spike'): Promise<StepResult> {
  return postJson<StepResult>('/demo/step', { preset })
}

/** Resets village yields back to standard DefiLlama baselines. */
export async function resetRealRates(): Promise<ResetResult> {
  return postJson<ResetResult>('/demo/reset', {})
}
