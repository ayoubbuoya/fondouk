import { describe, expect, it, vi } from 'vitest'

import { EventBus } from './townBus'

describe('EventBus', () => {
  it('calls every listener of an event with the payload, in the order added', () => {
    const bus = new EventBus()
    const calls: string[] = []
    bus.on('x', (value: number) => calls.push(`a${value}`))
    bus.on('x', (value: number) => calls.push(`b${value}`))
    expect(bus.emit('x', 7)).toBe(true)
    expect(calls).toEqual(['a7', 'b7'])
  })

  it('does not call listeners of another event', () => {
    const bus = new EventBus()
    const listener = vi.fn()
    bus.on('x', listener)
    expect(bus.emit('y')).toBe(false)
    expect(listener).not.toHaveBeenCalled()
  })

  it('stops calling a listener after off', () => {
    const bus = new EventBus()
    const kept = vi.fn()
    const removed = vi.fn()
    bus.on('x', kept).on('x', removed)
    bus.off('x', removed)
    bus.emit('x')
    expect(kept).toHaveBeenCalledOnce()
    expect(removed).not.toHaveBeenCalled()
  })

  it('calls a once listener a single time', () => {
    const bus = new EventBus()
    const listener = vi.fn()
    bus.once('x', listener)
    bus.emit('x', 1)
    bus.emit('x', 2)
    expect(listener).toHaveBeenCalledExactlyOnceWith(1)
  })

  it('removes a once listener with off before it fires (TownCanvas unmount)', () => {
    const bus = new EventBus()
    const listener = vi.fn()
    bus.once('town:ready', listener)
    bus.off('town:ready', listener)
    expect(bus.emit('town:ready')).toBe(false)
    expect(listener).not.toHaveBeenCalled()
  })

  it('lets a listener remove itself while the event is being sent', () => {
    const bus = new EventBus()
    const after = vi.fn()
    const selfRemoving = () => bus.off('x', selfRemoving)
    bus.on('x', selfRemoving).on('x', after)
    bus.emit('x')
    bus.emit('x')
    expect(after).toHaveBeenCalledTimes(2)
  })

  it('ignores off for a listener that was never added', () => {
    const bus = new EventBus()
    expect(() => bus.off('x', () => {})).not.toThrow()
  })
})
