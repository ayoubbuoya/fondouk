/**
 * Client for the backend's `/ws` live events (F8.5, HANDOFF D43).
 *
 * The server only pushes; the browser never sends. Messages are JSON with a
 * `type` field. Unknown or broken messages are ignored, so a newer backend
 * cannot crash an older page.
 */

import type { Village } from './api'

/** Fixed wait before reconnecting after the socket closes (D49). */
const RECONNECT_DELAY_MS = 5_000

export type LiveEvent =
  | { type: 'apr_update'; villages: Village[] }
  | {
      type: 'decision'
      avatar_id: string
      ts: string
      action: unknown
      valid: boolean
      rejected_rule: string | null
      reason_public: string | null
      cost_usd: string
      equity_usd: string
    }
  | { type: 'avatar_move'; avatar_id: string; from: string | null; to: string | null }
  | {
      type: 'arena_message'
      id: number
      avatar_id: string
      avatar_name: string
      sprite: string
      text: string
      ts: string
    }
  | {
      /** A character reached a level or unlocked a badge (D74); sent once each. */
      type: 'milestone'
      avatar_id: string
      avatar_name: string
      /** As stored: `level:4` or `badge:first_steps`. */
      milestone: string
      kind: 'level' | 'badge'
      /** Ready to show: "Level 4" or "First Steps". */
      label: string
      /** The character's level after the tick, for its name tag. */
      level: number
      ts: string
    }

/**
 * Turns one WebSocket text frame into an event, or `null` if it is not JSON
 * or not a type we know. Only the fields the UI relies on are checked.
 */
export function parseLiveEvent(text: string): LiveEvent | null {
  let data: unknown
  try {
    data = JSON.parse(text)
  } catch {
    return null
  }
  if (typeof data !== 'object' || data === null) return null
  const event = data as Record<string, unknown>

  switch (event.type) {
    case 'apr_update':
      return Array.isArray(event.villages) ? (event as LiveEvent) : null
    case 'decision':
      return typeof event.avatar_id === 'string' ? (event as LiveEvent) : null
    case 'avatar_move':
      return typeof event.avatar_id === 'string' ? (event as LiveEvent) : null
    case 'arena_message':
      return typeof event.id === 'number' &&
        typeof event.avatar_id === 'string' &&
        typeof event.text === 'string'
        ? (event as LiveEvent)
        : null
    case 'milestone':
      return typeof event.avatar_id === 'string' &&
        typeof event.avatar_name === 'string' &&
        typeof event.milestone === 'string' &&
        (event.kind === 'level' || event.kind === 'badge') &&
        typeof event.label === 'string' &&
        typeof event.level === 'number'
        ? (event as LiveEvent)
        : null
    default:
      return null
  }
}

/** `VITE_WS_URL`, or the same origin through the dev proxy (`/api/ws`). */
function liveEventsUrl(): string {
  const configured = import.meta.env.VITE_WS_URL
  if (configured) return configured
  const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws'
  return `${scheme}://${window.location.host}/api/ws`
}

/**
 * Connects to `/ws` and calls `onEvent` for each known event. If the socket
 * closes (backend restart, network drop) it reconnects after 5 s, so live
 * labels recover without a page reload (F8.5).
 *
 * @returns a function that closes the socket and stops reconnecting.
 */
export function connectLiveEvents(onEvent: (event: LiveEvent) => void): () => void {
  let socket: WebSocket | null = null
  let retryTimer: ReturnType<typeof setTimeout> | undefined
  let stopped = false

  const open = () => {
    socket = new WebSocket(liveEventsUrl())
    socket.onmessage = (message) => {
      if (typeof message.data !== 'string') return
      const event = parseLiveEvent(message.data)
      if (event) onEvent(event)
    }
    // `close` also follows a failed connection attempt, so this one handler
    // covers "backend down at page load" too.
    socket.onclose = () => {
      if (!stopped) retryTimer = setTimeout(open, RECONNECT_DELAY_MS)
    }
  }

  open()
  return () => {
    stopped = true
    clearTimeout(retryTimer)
    socket?.close()
  }
}
