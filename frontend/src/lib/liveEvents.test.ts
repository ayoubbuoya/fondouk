import { describe, expect, it } from 'vitest'

import { parseLiveEvent } from './liveEvents'

describe('parseLiveEvent', () => {
  it('parses an apr_update with its villages', () => {
    const text = JSON.stringify({
      type: 'apr_update',
      villages: [{ id: 'rhea-lend-usdc', apy_base: '14.01', stale: false }],
    })

    const event = parseLiveEvent(text)

    expect(event?.type).toBe('apr_update')
    if (event?.type !== 'apr_update') throw new Error('wrong type')
    expect(event.villages).toHaveLength(1)
    expect(event.villages[0].id).toBe('rhea-lend-usdc')
  })

  it('parses an avatar_move (used by the sprites on Day 7)', () => {
    const text = '{"type":"avatar_move","avatar_id":"a1","from":null,"to":"rhea-lend-usdc"}'

    expect(parseLiveEvent(text)).toEqual({
      type: 'avatar_move',
      avatar_id: 'a1',
      from: null,
      to: 'rhea-lend-usdc',
    })
  })

  it('ignores unknown event types', () => {
    expect(parseLiveEvent('{"type":"arena_message","text":"hi"}')).toBeNull()
  })

  it('ignores broken JSON and non-objects', () => {
    expect(parseLiveEvent('{not json')).toBeNull()
    expect(parseLiveEvent('42')).toBeNull()
    expect(parseLiveEvent('null')).toBeNull()
  })

  it('ignores a known type with the wrong shape', () => {
    expect(parseLiveEvent('{"type":"apr_update","villages":"oops"}')).toBeNull()
  })
})
