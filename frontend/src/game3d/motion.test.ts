import { describe, expect, it } from 'vitest'

import { apyGlowLevels, arcPoint, GESTURE_WAIT, idleGestureDelay, smokePuff, swayAngle } from './motion'

describe('arcPoint', () => {
  const from = { x: 0, y: 1, z: 0 }
  const to = { x: 10, y: 2, z: -4 }

  it('starts and lands exactly on its two points', () => {
    expect(arcPoint(from, to, 3, 0)).toEqual(from)
    expect(arcPoint(from, to, 3, 1)).toEqual(to)
  })

  it('is at its highest point, the full hump above the line, in the middle', () => {
    const middle = arcPoint(from, to, 3, 0.5)
    expect(middle.x).toBeCloseTo(5)
    expect(middle.z).toBeCloseTo(-2)
    expect(middle.y).toBeCloseTo(1.5 + 3)
    expect(arcPoint(from, to, 3, 0.3).y).toBeLessThan(middle.y)
    expect(arcPoint(from, to, 3, 0.7).y).toBeLessThan(middle.y)
  })

  it('stays on its end points for times outside the flight', () => {
    expect(arcPoint(from, to, 3, -1)).toEqual(from)
    expect(arcPoint(from, to, 3, 2)).toEqual(to)
  })
})

describe('apyGlowLevels', () => {
  const village = (id: string, apy: string | null, stale = false) => ({
    id,
    apy_base: apy,
    apy_reward: '0',
    stale,
  })

  it('gives the best village 1 and the others their share of it', () => {
    const levels = apyGlowLevels([village('a', '8'), village('b', '4'), village('c', '2')])
    expect(levels.get('a')).toBe(1)
    expect(levels.get('b')).toBeCloseTo(0.5)
    expect(levels.get('c')).toBeCloseTo(0.25)
  })

  it('counts the reward APY, like the signs do', () => {
    const levels = apyGlowLevels([
      { id: 'a', apy_base: '2', apy_reward: '6', stale: false },
      village('b', '4'),
    ])
    expect(levels.get('a')).toBe(1)
    expect(levels.get('b')).toBeCloseTo(0.5)
  })

  it('gives no glow to a stale village, one without data, or one at 0 % or below', () => {
    const levels = apyGlowLevels([village('best', '5'), village('stale', '50', true), village('none', null), village('neg', '-1')])
    expect(levels.get('best')).toBe(1)
    expect(levels.get('stale')).toBe(0)
    expect(levels.get('none')).toBe(0)
    expect(levels.get('neg')).toBe(0)
  })

  it('gives every village 0 when none has a positive APY (no division by 0)', () => {
    const levels = apyGlowLevels([village('a', '0'), village('b', null)])
    expect([...levels.values()]).toEqual([0, 0])
  })
})

describe('swayAngle', () => {
  it('never turns further than its amplitude', () => {
    for (let seconds = 0; seconds < 10; seconds += 0.1) {
      expect(Math.abs(swayAngle(seconds, 1.3, 0.1, 4))).toBeLessThanOrEqual(0.1 + 1e-12)
    }
  })

  it('moves two parts with different phases out of step', () => {
    expect(swayAngle(1, 0, 0.1, 4)).not.toBeCloseTo(swayAngle(1, 2, 0.1, 4))
  })
})

describe('smokePuff', () => {
  it('rises and grows as it gets older', () => {
    const young = smokePuff(0.5, 4)
    const old = smokePuff(3, 4)
    expect(old.rise).toBeGreaterThan(young.rise)
    expect(old.scale).toBeGreaterThan(young.scale)
  })

  it('is invisible when it leaves the chimney and at the end of its life', () => {
    expect(smokePuff(0, 4).opacity).toBe(0)
    expect(smokePuff(4, 4).opacity).toBe(0)
    expect(smokePuff(1, 4).opacity).toBeGreaterThan(0)
  })
})

describe('idleGestureDelay', () => {
  it('always waits between the shortest and the longest time', () => {
    expect(idleGestureDelay(0)).toBe(GESTURE_WAIT.min)
    expect(idleGestureDelay(1)).toBe(GESTURE_WAIT.max)
    expect(idleGestureDelay(-5)).toBe(GESTURE_WAIT.min)
    expect(idleGestureDelay(5)).toBe(GESTURE_WAIT.max)
    const middle = idleGestureDelay(0.5)
    expect(middle).toBeGreaterThan(GESTURE_WAIT.min)
    expect(middle).toBeLessThan(GESTURE_WAIT.max)
  })
})
