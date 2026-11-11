import { describe, expect, it } from 'vitest'

import {
  clampToRect,
  distanceLimits,
  easeInOut,
  fitDistance,
  IDLE_SWAY,
  idleSway,
  lerpPose,
  MAX_TURN,
  MIN_DISTANCE,
  openingPose,
  poseOffset,
  START_TILT,
  TILT_LIMITS,
  type CameraPose,
} from './cameraRig'

const TOWN = { w: 60, d: 40 }
const NO_BARS = { top: 0, bottom: 0 }

describe('fitDistance', () => {
  it('moves back when the window gets narrower', () => {
    const wide = fitDistance(TOWN, { w: 1600, h: 900 }, NO_BARS, START_TILT)
    const narrow = fitDistance(TOWN, { w: 800, h: 900 }, NO_BARS, START_TILT)
    expect(narrow).toBeGreaterThan(wide)
  })

  it('moves back when the HUD bars cover part of a wide, short view', () => {
    const view = { w: 2400, h: 500 }
    const without = fitDistance(TOWN, view, NO_BARS, START_TILT)
    const withBars = fitDistance(TOWN, view, { top: 60, bottom: 36 }, START_TILT)
    expect(withBars).toBeGreaterThan(without)
  })

  it('needs less distance for the depth when tilted more (the depth looks shorter)', () => {
    const view = { w: 3000, h: 600 } // depth is the tight side
    expect(fitDistance(TOWN, view, NO_BARS, 1.1)).toBeLessThan(fitDistance(TOWN, view, NO_BARS, 0.4))
  })

  it('never returns less than the minimum, even for a view that is not laid out yet', () => {
    expect(fitDistance(TOWN, { w: 0, h: 0 }, NO_BARS, START_TILT)).toBe(MIN_DISTANCE)
    expect(fitDistance({ w: 1, d: 1 }, { w: 1000, h: 1000 }, NO_BARS, START_TILT)).toBe(MIN_DISTANCE)
  })
})

describe('distanceLimits', () => {
  it('lets the user zoom in to the minimum and out a little past the start', () => {
    const limits = distanceLimits(60)
    expect(limits.min).toBe(MIN_DISTANCE)
    expect(limits.max).toBeGreaterThan(60)
  })

  it('keeps min below max when the start is already very close', () => {
    const limits = distanceLimits(MIN_DISTANCE / 2)
    expect(limits.min).toBeLessThanOrEqual(limits.max)
  })
})

describe('clampToRect', () => {
  const rect = { x: 10, z: 5, w: 60, d: 40 }

  it('keeps a point that is inside', () => {
    expect(clampToRect(20, 20, rect)).toEqual({ x: 20, z: 20 })
  })

  it('pulls a point outside back to the edge', () => {
    expect(clampToRect(0, 100, rect)).toEqual({ x: 10, z: 45 })
    expect(clampToRect(999, -3, rect)).toEqual({ x: 70, z: 5 })
  })
})

describe('poseOffset', () => {
  const target = { x: 0, z: 0 }

  it('puts the camera straight above the target when not tilted', () => {
    const offset = poseOffset({ target, distance: 10, tilt: 0, turn: 0 })
    expect(offset.x).toBeCloseTo(0)
    expect(offset.y).toBeCloseTo(10)
    expect(offset.z).toBeCloseTo(0)
  })

  it('puts a tilted, unturned camera south of the target (larger z)', () => {
    const offset = poseOffset({ target, distance: 10, tilt: Math.PI / 2, turn: 0 })
    expect(offset.x).toBeCloseTo(0)
    expect(offset.y).toBeCloseTo(0)
    expect(offset.z).toBeCloseTo(10)
  })

  it('keeps the camera at `distance` from the target, whatever the angles', () => {
    const offset = poseOffset({ target, distance: 30, tilt: 0.8, turn: -0.5 })
    expect(Math.hypot(offset.x, offset.y, offset.z)).toBeCloseTo(30)
  })
})

describe('lerpPose', () => {
  const from: CameraPose = { target: { x: 0, z: 10 }, distance: 16, tilt: 1.1, turn: -0.5 }
  const to: CameraPose = { target: { x: 30, z: 20 }, distance: 70, tilt: 0.8, turn: 0 }

  it('starts and ends exactly on its two views', () => {
    expect(lerpPose(from, to, 0)).toEqual(from)
    expect(lerpPose(from, to, 1)).toEqual(to)
  })

  it('is half-way in every number at the middle', () => {
    const middle = lerpPose(from, to, 0.5)
    expect(middle.target).toEqual({ x: 15, z: 15 })
    expect(middle.distance).toBeCloseTo(43)
    expect(middle.tilt).toBeCloseTo(0.95)
    expect(middle.turn).toBeCloseTo(-0.25)
  })
})

describe('openingPose', () => {
  const gate = { x: 30, z: 37 }

  it('looks at the gate, inside every limit the controls enforce', () => {
    const limits = distanceLimits(70)
    const pose = openingPose(gate, limits)
    expect(pose.target).toEqual(gate)
    expect(pose.distance).toBeGreaterThanOrEqual(limits.min)
    expect(pose.distance).toBeLessThanOrEqual(limits.max)
    expect(pose.tilt).toBeGreaterThanOrEqual(TILT_LIMITS.min)
    expect(pose.tilt).toBeLessThan(TILT_LIMITS.max)
    expect(Math.abs(pose.turn)).toBeLessThan(MAX_TURN)
  })

  it('stays inside the zoom limits of a very small window', () => {
    const limits = distanceLimits(MIN_DISTANCE) // max is only a little past the minimum
    const pose = openingPose(gate, limits)
    expect(pose.distance).toBeLessThanOrEqual(limits.max)
    expect(pose.distance).toBeGreaterThanOrEqual(limits.min)
  })
})

describe('idleSway', () => {
  it('starts with no turn, so the sway never begins with a jump', () => {
    expect(idleSway(0)).toBe(0)
  })

  it('never turns further than its amplitude, and reaches it', () => {
    let furthest = 0
    for (let seconds = 0; seconds <= IDLE_SWAY.period; seconds += 0.25) {
      furthest = Math.max(furthest, Math.abs(idleSway(seconds)))
    }
    expect(furthest).toBeLessThanOrEqual(IDLE_SWAY.amplitude)
    expect(furthest).toBeCloseTo(IDLE_SWAY.amplitude)
  })

  it('is back at the start after one full period', () => {
    expect(idleSway(IDLE_SWAY.period)).toBeCloseTo(0)
  })
})

describe('easeInOut', () => {
  it('goes from 0 to 1 through the middle', () => {
    expect(easeInOut(0)).toBe(0)
    expect(easeInOut(0.5)).toBeCloseTo(0.5)
    expect(easeInOut(1)).toBe(1)
  })

  it('stays inside 0..1 for times outside the move', () => {
    expect(easeInOut(-1)).toBe(0)
    expect(easeInOut(2)).toBe(1)
  })
})
