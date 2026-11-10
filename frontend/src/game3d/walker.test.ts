import { describe, expect, it } from 'vitest'

import { walkAt } from './walker'

const FROM = { x: 0, y: 0 }
/** 100 px east, then 50 px south: 150 px in total. */
const PATH = [
  { x: 100, y: 0 },
  { x: 100, y: 50 },
]
const SPEED = 50 // px per second -> the walk takes 3 s

describe('walkAt', () => {
  it('starts at the start, already facing the first step', () => {
    const moment = walkAt(FROM, PATH, SPEED, 0)
    expect(moment.position).toEqual(FROM)
    expect(moment.heading).toEqual({ x: 100, y: 0 })
    expect(moment.done).toBe(false)
  })

  it('is part-way along the first step', () => {
    expect(walkAt(FROM, PATH, SPEED, 1).position).toEqual({ x: 50, y: 0 })
  })

  it('turns the corner onto the second step', () => {
    const moment = walkAt(FROM, PATH, SPEED, 2.5)
    expect(moment.position).toEqual({ x: 100, y: 25 })
    expect(moment.heading).toEqual({ x: 0, y: 50 })
    expect(moment.done).toBe(false)
  })

  it('stops on the last point, still facing the last step', () => {
    for (const elapsed of [3, 10]) {
      const moment = walkAt(FROM, PATH, SPEED, elapsed)
      expect(moment.position).toEqual({ x: 100, y: 50 })
      expect(moment.heading).toEqual({ x: 0, y: 50 })
      expect(moment.done).toBe(true)
    }
  })

  it('is done at once for an empty path, with no heading', () => {
    expect(walkAt(FROM, [], SPEED, 1)).toEqual({ position: FROM, heading: null, done: true })
  })

  it('treats a negative time as the start', () => {
    expect(walkAt(FROM, PATH, SPEED, -5).position).toEqual(FROM)
  })

  it('skips a zero-length step without losing the heading', () => {
    const twice = [
      { x: 10, y: 0 },
      { x: 10, y: 0 },
    ]
    expect(walkAt(FROM, twice, 10, 5)).toEqual({ position: { x: 10, y: 0 }, heading: { x: 10, y: 0 }, done: true })
  })
})
