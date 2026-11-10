/**
 * The news strip at the bottom of the town: one short line per public event.
 * Pure, no DOM: unit-tested.
 *
 * Only public data is used (the same fields the panels show): a decision's
 * action and the rule that refused it, and arena messages. The persona never
 * reaches the browser, so it cannot appear here (D7).
 */

import type { ArenaMessage } from './api'
import type { LiveEvent } from './liveEvents'

/** How the line is coloured. */
export type TickerTone = 'move' | 'hold' | 'blocked' | 'say'

export interface TickerLine {
  /** Stable per event, so a line is never shown twice. */
  id: string
  ts: string
  /** Clicking the line opens this character. */
  avatarId: string
  text: string
  tone: TickerTone
}

/** The strip keeps this many lines; older ones drop off. */
export const TICKER_MAX_LINES = 20

/** Names for ids, with a fallback so a line is never empty. */
export interface TickerNames {
  avatars: Map<string, string>
  villages: Map<string, string>
}

const UNKNOWN_AVATAR = 'Someone'

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

/** A village's display name, or its id when the town does not know it. */
function villageName(names: TickerNames, id: unknown): string {
  return typeof id === 'string' ? (names.villages.get(id) ?? id) : '?'
}

/** The line for one arena message (live, or from `GET /arena/feed` at load). */
export function arenaLine(
  message: Pick<ArenaMessage, 'id' | 'avatar_id' | 'avatar_name' | 'text' | 'ts'>,
): TickerLine {
  return {
    id: `arena-${message.id}`,
    ts: message.ts,
    avatarId: message.avatar_id,
    text: `${message.avatar_name}: “${message.text}”`,
    tone: 'say',
  }
}

/**
 * The line for one live event, or `null` when the event has no line:
 * - `apr_update`: the signs on the map already show it;
 * - `avatar_move`: the `decision` that caused it already has a line;
 * - a `SAY` decision: the `arena_message` that follows has the text.
 */
export function tickerLine(event: LiveEvent, names: TickerNames): TickerLine | null {
  if (event.type === 'arena_message') return arenaLine(event)
  if (event.type !== 'decision') return null

  const name = names.avatars.get(event.avatar_id) ?? UNKNOWN_AVATAR
  const line = (text: string, tone: TickerTone): TickerLine => ({
    id: `decision-${event.avatar_id}-${event.ts}`,
    ts: event.ts,
    avatarId: event.avatar_id,
    text,
    tone,
  })

  if (!event.valid) {
    return line(`${name} was blocked by guardrail ${event.rejected_rule ?? '?'}`, 'blocked')
  }
  const action = isRecord(event.action) ? event.action : {}
  switch (action.action) {
    case 'MOVE': {
      const from = villageName(names, action.from)
      const to = villageName(names, action.to)
      return line(`${name} moved ${String(action.percent ?? '?')}% from ${from} to ${to}`, 'move')
    }
    case 'REBALANCE':
      return line(`${name} rebalanced the portfolio`, 'move')
    case 'HOLD':
      return line(`${name} is holding`, 'hold')
    default:
      return null
  }
}

/**
 * Adds `line` in front (newest first). A line already in the list is not
 * added again, and the list never grows past `TICKER_MAX_LINES`.
 */
export function pushLine(lines: TickerLine[], line: TickerLine): TickerLine[] {
  if (lines.some((existing) => existing.id === line.id)) return lines
  return [line, ...lines].slice(0, TICKER_MAX_LINES)
}
