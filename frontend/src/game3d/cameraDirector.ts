/**
 * The camera operator of the 3D town (D79): every camera move that is *not*
 * the user's own mouse. `createTown3d.ts` owns the camera and the controls
 * and calls `update` once per frame; the maths is in `cameraRig.ts`.
 *
 * ```text
 *              first size known                 any pointer input
 *   ┌─────────┐  FLY_IN_SECONDS  ┌──────┐ IDLE_SECONDS ┌──────┐
 *   │ FLY-IN  ├─────────────────►│ FREE ├─────────────►│ SWAY ├──► FREE
 *   └────┬────┘                  └──────┘              └──────┘
 *        │ click or wheel = skip to the end
 *
 *   "camera:focus"    (Judge Tour)      ─► GLIDE to a place, or back home
 *   "avatar:selected" (a click)         ─► GLIDE to the character, then FOLLOW it
 *   FOLLOW ends when the user drags the town or a "camera:focus" arrives.
 * ```
 *
 * One rule shapes this file: the controls clamp the camera to their limits
 * every frame, even while disabled. So scripted moves only ever go *inside*
 * those limits (see `openingPose`), and the user's input is switched off
 * while a move runs so the two never fight.
 */

import { Vector3, type PerspectiveCamera } from 'three'
import type { MapControls } from 'three/addons/controls/MapControls.js'

import { townBus, type CameraFocus } from '../game/townBus'
import {
  easeInOut,
  FLY_IN_SECONDS,
  FOCUS_DISTANCE,
  IDLE_SECONDS,
  idleSway,
  lerpPose,
  openingPose,
  poseOffset,
  type CameraPose,
} from './cameraRig'
import type { GroundPoint } from './coords'
import type { Town3d } from './Town3d'

const GLIDE_SECONDS = 0.6
const UP = new Vector3(0, 1, 0)

/** A short move from where the camera is to a new view. */
interface Glide {
  fromTarget: Vector3
  toTarget: Vector3
  /** Camera position minus target. */
  fromOffset: Vector3
  toOffset: Vector3
  elapsed: number
}

export interface CameraDirectorOptions {
  camera: PerspectiveCamera
  controls: MapControls
  /** Knows where characters, villages and landmarks are. */
  town: Town3d
  /** The element that receives the user's pointer input (canvas and labels). */
  element: HTMLElement
  /** Where the opening flight starts. */
  gate: GroundPoint
}

export interface CameraDirector {
  /**
   * Tells the director the whole-town view and the zoom limits. Call at start
   * and on every resize. The first call starts the opening flight.
   */
  setHome(home: CameraPose, limits: { min: number; max: number }): void
  /** Advances the running move, if any. Call once per frame, before `controls.update()`. */
  update(deltaSeconds: number): void
  /** Stops listening. The caller frees the camera and the controls. */
  dispose(): void
}

export function createCameraDirector(options: CameraDirectorOptions): CameraDirector {
  const { camera, controls, town, element, gate } = options
  // Visitors who asked their system for less motion get no flight and no sway.
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

  let home: CameraPose | undefined
  let limits = { min: 0, max: Infinity }
  /** Seconds into the opening flight, while it runs. */
  let flyInElapsed: number | undefined
  let glide: Glide | undefined
  /** The character the camera follows, and where it was last frame. */
  let follow: { avatarId: string; point: GroundPoint } | undefined
  let idleSeconds = 0
  /** Seconds into the sway, and the turn it has added so far. */
  let sway: { seconds: number; applied: number } | undefined

  /** Puts the camera on a view at once. */
  const place = (pose: CameraPose) => {
    const offset = poseOffset(pose)
    controls.target.set(pose.target.x, 0, pose.target.z)
    camera.position.set(pose.target.x + offset.x, offset.y, pose.target.z + offset.z)
  }

  const endFlyIn = () => {
    flyInElapsed = undefined
    controls.enabled = true
    idleSeconds = 0
  }

  const stepFlyIn = (deltaSeconds: number) => {
    if (flyInElapsed === undefined || !home) return
    flyInElapsed += deltaSeconds
    // Recomputed every frame, so a resize during the flight still lands on the right view.
    place(lerpPose(openingPose(gate, limits), home, easeInOut(flyInElapsed / FLY_IN_SECONDS)))
    if (flyInElapsed >= FLY_IN_SECONDS) endFlyIn()
  }

  /**
   * Starts a glide to `point`, ending `distance` away from it at the same
   * viewing angle, or back to the home view for `null`.
   */
  const startGlide = (point: GroundPoint | null, distance: number) => {
    if (!home) return
    flyInElapsed = undefined // a glide takes over from the opening flight
    sway = undefined
    idleSeconds = 0
    const fromOffset = camera.position.clone().sub(controls.target)
    const homeOffset = poseOffset(home)
    glide = {
      fromTarget: controls.target.clone(),
      fromOffset,
      toTarget: point ? new Vector3(point.x, 0, point.z) : new Vector3(home.target.x, 0, home.target.z),
      toOffset: point
        ? fromOffset.clone().setLength(distance)
        : new Vector3(homeOffset.x, homeOffset.y, homeOffset.z),
      elapsed: 0,
    }
    controls.enabled = false // the user's drag must not fight the glide
  }

  const stepGlide = (deltaSeconds: number) => {
    if (!glide) return
    glide.elapsed += deltaSeconds
    const progress = easeInOut(glide.elapsed / GLIDE_SECONDS)
    controls.target.lerpVectors(glide.fromTarget, glide.toTarget, progress)
    camera.position.lerpVectors(glide.fromOffset, glide.toOffset, progress).add(controls.target)
    if (glide.elapsed >= GLIDE_SECONDS) {
      glide = undefined
      controls.enabled = true
    }
  }

  /**
   * Keeps the followed character where it is on screen: the camera and the
   * point it looks at move by the same step the character took. Moving by the
   * *step* (not snapping to the character) leaves the user's zoom and turn alone.
   */
  const stepFollow = () => {
    if (!follow) return
    const point = town.focusPoint({ avatarId: follow.avatarId })
    if (!point) {
      follow = undefined // the character left the town
      return
    }
    const dx = point.x - follow.point.x
    const dz = point.z - follow.point.z
    follow.point = point
    if (glide) {
      // Still gliding to it: aim at where it is now, not where it was.
      glide.toTarget.set(point.x, 0, point.z)
      return
    }
    controls.target.x += dx
    controls.target.z += dz
    camera.position.x += dx
    camera.position.z += dz
  }

  const stepSway = (deltaSeconds: number) => {
    if (reducedMotion) return
    idleSeconds += deltaSeconds
    if (idleSeconds < IDLE_SECONDS) return
    sway ??= { seconds: 0, applied: 0 }
    sway.seconds += deltaSeconds
    const wanted = idleSway(sway.seconds)
    // Turn the camera around the point it looks at by the change since last frame.
    const offset = camera.position.clone().sub(controls.target).applyAxisAngle(UP, wanted - sway.applied)
    camera.position.copy(controls.target).add(offset)
    sway.applied = wanted
  }

  // ---- what the user and the app ask for ----

  /** Any pointer activity: the view is in use, so stop swaying and wait again. */
  const onActivity = () => {
    idleSeconds = 0
    // The view stays where the sway left it: turning back would be a jump.
    sway = undefined
  }
  const onPointerDown = (event: PointerEvent) => {
    onActivity()
    if (flyInElapsed !== undefined && home) {
      // Never make the user wait for the flight: one click ends it.
      place(home)
      endFlyIn()
    }
    // A left press starts a drag across the town: the user takes the camera back.
    if (event.button === 0) follow = undefined
  }
  const onWheel = () => {
    onActivity()
    if (flyInElapsed !== undefined && home) {
      place(home)
      endFlyIn()
    }
  }
  element.addEventListener('pointerdown', onPointerDown)
  element.addEventListener('pointermove', onActivity)
  element.addEventListener('wheel', onWheel, { passive: true })

  /** Glides to a character, a village, a landmark or back home (used by the Judge Tour). */
  const onFocus = (focus: CameraFocus) => {
    const point = focus.reset ? null : town.focusPoint(focus)
    if (!focus.reset && !point) return // its target is not in the town
    follow = undefined
    startGlide(point, FOCUS_DISTANCE)
  }
  /** A character was selected (clicked in the town): go to it and stay with it. */
  const onAvatarSelected = (avatarId: string) => {
    const point = town.focusPoint({ avatarId })
    if (!point) return
    follow = { avatarId, point }
    // Come closer if far away, but never push back a user who already zoomed in.
    startGlide(point, Math.min(camera.position.distanceTo(controls.target), FOCUS_DISTANCE))
  }
  townBus.on('camera:focus', onFocus)
  townBus.on('avatar:selected', onAvatarSelected)

  return {
    setHome(nextHome, nextLimits) {
      const first = home === undefined
      home = nextHome
      limits = nextLimits
      if (!first) return // later calls are resizes: the user's view is kept
      if (reducedMotion) {
        place(nextHome)
      } else {
        flyInElapsed = 0
        controls.enabled = false
        place(openingPose(gate, nextLimits))
      }
      controls.update()
    },

    update(deltaSeconds) {
      if (flyInElapsed !== undefined) {
        stepFlyIn(deltaSeconds)
        return
      }
      stepFollow()
      if (glide) stepGlide(deltaSeconds)
      else stepSway(deltaSeconds)
    },

    dispose() {
      element.removeEventListener('pointerdown', onPointerDown)
      element.removeEventListener('pointermove', onActivity)
      element.removeEventListener('wheel', onWheel)
      townBus.off('camera:focus', onFocus)
      townBus.off('avatar:selected', onAvatarSelected)
    },
  }
}
