/**
 * The numbers behind the 3D camera. Pure, no three.js: unit-tested.
 * `createTown3d.ts` feeds them to three's `MapControls`.
 *
 * ```text
 *   side view                         "tilt" = angle from straight down
 *        camera ◉                       0      = a flat map from above
 *              ╱ ╲  fov                 π / 2  = standing on the ground
 *    distance ╱   ╲
 *            ╱tilt ╲
 *   ────────●───────────  ground      ● = the point the camera looks at
 * ```
 */

/** Vertical field of view of the camera, in degrees. */
export const FOV_DEG = 40

/** How far the user may tilt, in radians from straight down. */
export const TILT_LIMITS = { min: 0.3, max: 1.15 } as const
/** The tilt the town opens with: clearly 3D, but every district sign readable. */
export const START_TILT = 0.8

/** The camera never comes closer to the ground point it looks at than this (units = tiles). */
export const MIN_DISTANCE = 9
/** The user may zoom out a little past the opening view, to see the desert. */
const MAX_DISTANCE_FACTOR = 1.35
/** Empty space kept around the town in the opening view. */
const FIT_MARGIN = 1.06

/** How far the user may turn the town, in radians: house fronts face south, so never show their backs. */
export const MAX_TURN = 0.9

/** Distance used when the Judge Tour focuses on one building or character. */
export const FOCUS_DISTANCE = 22

/** How long the opening flight from the gate to the whole-town view takes. */
export const FLY_IN_SECONDS = 4
/** Seconds without any mouse or touch input before the view starts to sway. */
export const IDLE_SECONDS = 20
/** The idle sway: a slow turn left and right. `amplitude` in radians, `period` in seconds. */
export const IDLE_SWAY = { amplitude: 0.22, period: 36 } as const

/** Where the opening flight starts: close to the gate, low, and turned a little to one side. */
const OPENING = { distance: 16, turn: -0.55, tiltMargin: 0.02 } as const

export interface ViewSize {
  w: number
  h: number
}

/** Screen pixels covered by the HUD bar (top) and the news strip (bottom). */
export interface Insets {
  top: number
  bottom: number
}

/**
 * The camera distance at which a `town.w` × `town.d` rectangle (in units)
 * fits in the part of the view no bar covers.
 *
 * Two things can be the tight one: the town's width against the view's
 * width, or its depth against the free height. Seen at an angle, the depth
 * looks shorter by `cos(tilt)`. The larger distance wins.
 */
export function fitDistance(town: { w: number; d: number }, view: ViewSize, insets: Insets, tilt: number): number {
  if (view.w <= 0 || view.h <= 0) return MIN_DISTANCE
  const halfHeightAtOne = Math.tan(((FOV_DEG / 2) * Math.PI) / 180)
  const aspect = view.w / view.h
  // Never let the bars take the whole view (a very short window).
  const freeShare = Math.max(0.2, (view.h - insets.top - insets.bottom) / view.h)

  const forWidth = town.w / 2 / (halfHeightAtOne * aspect)
  const forDepth = ((town.d / 2) * Math.cos(tilt)) / (halfHeightAtOne * freeShare)
  return Math.max(MIN_DISTANCE, Math.max(forWidth, forDepth) * FIT_MARGIN)
}

/** How close and how far the user may zoom, given the opening distance. */
export function distanceLimits(startDistance: number): { min: number; max: number } {
  return { min: Math.min(MIN_DISTANCE, startDistance), max: startDistance * MAX_DISTANCE_FACTOR }
}

/** Keeps the point the camera looks at inside a ground rectangle, so the user cannot pan into the void. */
export function clampToRect(
  x: number,
  z: number,
  rect: { x: number; z: number; w: number; d: number },
): { x: number; z: number } {
  return {
    x: Math.min(rect.x + rect.w, Math.max(rect.x, x)),
    z: Math.min(rect.z + rect.d, Math.max(rect.z, z)),
  }
}

/**
 * A full camera view, in the words of the picture at the top of this file.
 *
 * ```text
 *   top view            turn = 0   the camera is south of the target, looking north
 *        N              turn > 0   the camera has moved towards the east
 *        ●  target
 *       ╱│
 *      ╱ │  turn
 *     ◉  S
 * ```
 */
export interface CameraPose {
  /** The ground point the camera looks at. */
  target: { x: number; z: number }
  distance: number
  /** Angle from straight down, in radians. */
  tilt: number
  /** Angle around the up axis, in radians. */
  turn: number
}

/** Where the camera is, relative to the point it looks at (y is up). */
export function poseOffset(pose: CameraPose): { x: number; y: number; z: number } {
  const flat = pose.distance * Math.sin(pose.tilt)
  return {
    x: flat * Math.sin(pose.turn),
    y: pose.distance * Math.cos(pose.tilt),
    z: flat * Math.cos(pose.turn),
  }
}

/** The view a share `t` (0..1) of the way from `from` to `to`. */
export function lerpPose(from: CameraPose, to: CameraPose, t: number): CameraPose {
  const mix = (a: number, b: number) => a + (b - a) * t
  return {
    target: { x: mix(from.target.x, to.target.x), z: mix(from.target.z, to.target.z) },
    distance: mix(from.distance, to.distance),
    tilt: mix(from.tilt, to.tilt),
    turn: mix(from.turn, to.turn),
  }
}

/**
 * The first frame of the opening flight: looking at the gate from outside
 * the wall, as low as the user is allowed to tilt.
 *
 * It must stay inside the limits the controls enforce (`limits`,
 * `TILT_LIMITS`, `MAX_TURN`): the controls clamp the camera every frame, even
 * during a scripted move, so a pose outside them would jump.
 */
export function openingPose(gate: { x: number; z: number }, limits: { min: number; max: number }): CameraPose {
  return {
    target: { x: gate.x, z: gate.z },
    distance: Math.min(limits.max, Math.max(limits.min, OPENING.distance)),
    tilt: TILT_LIMITS.max - OPENING.tiltMargin,
    turn: OPENING.turn,
  }
}

/**
 * How far the idle view is turned away from where the sway began, `seconds`
 * after it began. Starts at 0, so the sway never begins with a jump.
 */
export function idleSway(seconds: number): number {
  return IDLE_SWAY.amplitude * Math.sin((seconds / IDLE_SWAY.period) * Math.PI * 2)
}

/** Smooth start and stop for camera moves: 0 -> 0, 1 -> 1, slow at both ends. */
export function easeInOut(t: number): number {
  const clamped = Math.min(1, Math.max(0, t))
  return clamped * clamped * (3 - 2 * clamped)
}
