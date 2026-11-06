/**
 * Typed calls to the backend REST API.
 *
 * Every path starts with `/api`: the Vite dev proxy (and later the Vercel
 * rewrite) removes that prefix and forwards to the backend, so the browser
 * talks to one origin and the HttpOnly session cookie is sent (D37, D47).
 *
 * Decimal values (APY, TVL) arrive as JSON **strings** such as `"14.01014"`.
 * They stay strings here; only display and drawing code turn them into numbers.
 */

/** One village, the shape of the backend's `VillageDto` (`GET /villages`). */
export interface Village {
  id: string
  name: string
  /** `lending`, `stable_lp`, ... */
  kind: string
  asset: string
  map_x: number
  map_y: number
  /** Percent per year (APY), `null` until the indexer stored a first point. */
  apy_base: string | null
  apy_reward: string | null
  tvl_usd: string | null
  /** ISO time of the point at the source (DefiLlama). */
  source_ts: string | null
  /** ISO time of the last successful indexer fetch. */
  last_fetched_at: string | null
  /** `true` = agents may not move money in; the map greys it out (F2.4). */
  stale: boolean
}

/** One point of `GET /villages/{id}/history`, oldest first. */
export interface YieldPoint {
  ts: string
  apy_base: string
  apy_reward: string
  tvl_usd: string
}

/** `POST /auth/challenge`: what the wallet must sign. */
export interface Challenge {
  /** base64 of 32 random bytes; single use, valid 5 minutes. */
  nonce: string
  message: string
  recipient: string
}

/** Body of `POST /auth/near` (backend `LoginRequest`). */
export interface LoginRequest {
  account_id: string
  /** `ed25519:<base58>` */
  public_key: string
  /** base64 of the 64-byte signature */
  signature: string
  /** exactly as the challenge returned it */
  nonce: string
}

/** A non-2xx answer. `message` is the backend's `{"error": ...}` text. */
export class ApiError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

/**
 * `fetch` + JSON + error handling. `same-origin` credentials (the default)
 * already send the session cookie because the API is same-origin.
 */
async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api${path}`, {
    method,
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!response.ok) {
    const error = await response.json().catch(() => null)
    throw new ApiError(response.status, error?.error ?? response.statusText)
  }
  // 204 No Content (logout) has no body.
  return (response.status === 204 ? undefined : await response.json()) as T
}

export function getVillages(): Promise<Village[]> {
  return request('GET', '/villages')
}

export function getVillageHistory(villageId: string, days: number): Promise<YieldPoint[]> {
  return request('GET', `/villages/${encodeURIComponent(villageId)}/history?days=${days}`)
}

export function createChallenge(): Promise<Challenge> {
  return request('POST', '/auth/challenge')
}

export function loginNear(body: LoginRequest): Promise<{ account_id: string }> {
  return request('POST', '/auth/near', body)
}

/** The logged-in account, or `null` when there is no valid session (401). */
export async function getMe(): Promise<string | null> {
  try {
    const me = await request<{ account_id: string }>('GET', '/auth/me')
    return me.account_id
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) return null
    throw error
  }
}

export function logoutSession(): Promise<void> {
  return request('POST', '/auth/logout')
}
