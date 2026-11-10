import { describe, expect, it } from 'vitest'

import type { LiveEvent } from './liveEvents'
import { arenaLine, pushLine, TICKER_MAX_LINES, tickerLine, type TickerLine, type TickerNames } from './ticker'

const names: TickerNames = {
  avatars: new Map([['a1', 'Amira']]),
  villages: new Map([
    ['rhea-lend-usdc', 'Rhea Lend USDC'],
    ['rhea-dex-stable4', 'Rhea Stable Pool'],
  ]),
}

const decision = (over: Partial<Extract<LiveEvent, { type: 'decision' }>>): LiveEvent => ({
  type: 'decision',
  avatar_id: 'a1',
  ts: '2026-11-10T10:00:00Z',
  action: { action: 'HOLD' },
  valid: true,
  rejected_rule: null,
  reason_public: null,
  cost_usd: '0',
  equity_usd: '1000',
  ...over,
})

describe('tickerLine', () => {
  it('describes a move with village names', () => {
    const line = tickerLine(
      decision({ action: { action: 'MOVE', from: 'rhea-lend-usdc', to: 'rhea-dex-stable4', percent: 25 } }),
      names,
    )
    expect(line?.text).toBe('Amira moved 25% from Rhea Lend USDC to Rhea Stable Pool')
    expect(line?.tone).toBe('move')
    expect(line?.avatarId).toBe('a1')
  })

  it('falls back to the village id when the town does not know it', () => {
    const line = tickerLine(decision({ action: { action: 'MOVE', from: 'x', to: 'y', percent: 10 } }), names)
    expect(line?.text).toBe('Amira moved 10% from x to y')
  })

  it('describes a rebalance and a hold', () => {
    expect(tickerLine(decision({ action: { action: 'REBALANCE', weights: {} } }), names)?.text).toBe(
      'Amira rebalanced the portfolio',
    )
    const hold = tickerLine(decision({}), names)
    expect(hold?.text).toBe('Amira is holding')
    expect(hold?.tone).toBe('hold')
  })

  it('names the guardrail when a proposal was refused', () => {
    const line = tickerLine(decision({ valid: false, rejected_rule: 'min_gain' }), names)
    expect(line?.text).toBe('Amira was blocked by guardrail min_gain')
    expect(line?.tone).toBe('blocked')
  })

  it('says "Someone" for a character the town has not loaded yet', () => {
    expect(tickerLine(decision({ avatar_id: 'new' }), names)?.text).toBe('Someone is holding')
  })

  it('quotes an arena message', () => {
    const message = { id: 7, avatar_id: 'a1', avatar_name: 'Amira', text: 'Stable wins.', ts: 't' }
    const line = tickerLine({ type: 'arena_message', sprite: 'boy', ...message }, names)
    expect(line).toEqual(arenaLine(message))
    expect(line?.text).toBe('Amira: “Stable wins.”')
    expect(line?.tone).toBe('say')
  })

  it('has no line for SAY decisions, moves, APY updates or unknown actions', () => {
    expect(tickerLine(decision({ action: { action: 'SAY', text: 'hi' } }), names)).toBeNull()
    expect(tickerLine(decision({ action: 'broken' }), names)).toBeNull()
    expect(tickerLine({ type: 'avatar_move', avatar_id: 'a1', from: null, to: 'x' }, names)).toBeNull()
    expect(tickerLine({ type: 'apr_update', villages: [] }, names)).toBeNull()
  })
})

describe('pushLine', () => {
  const line = (id: string): TickerLine => ({ id, ts: 't', avatarId: 'a1', text: id, tone: 'hold' })

  it('puts the newest line first', () => {
    expect(pushLine([line('old')], line('new')).map((l) => l.id)).toEqual(['new', 'old'])
  })

  it('does not add the same event twice', () => {
    const lines = [line('a')]
    expect(pushLine(lines, line('a'))).toBe(lines)
  })

  it('drops the oldest line past the limit', () => {
    let lines: TickerLine[] = []
    for (let i = 0; i < TICKER_MAX_LINES + 5; i++) lines = pushLine(lines, line(`l${i}`))
    expect(lines).toHaveLength(TICKER_MAX_LINES)
    expect(lines[0].id).toBe(`l${TICKER_MAX_LINES + 4}`)
  })
})
