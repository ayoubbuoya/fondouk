/**
 * Starts the 3D town inside `parent` (D76, D77).
 *
 * This file is the "stage crew": the renderer, the camera and its controls,
 * the click detection and the frame loop. What is *on* the stage is `Town3d`;
 * the evening light is `atmosphere.ts`, the glow is `postFx.ts` and the
 * scripted camera moves are `cameraDirector.ts` (D78, D79).
 *
 * ```text
 *   every frame:  town.update(dt)      walks and animations
 *                 director.update(dt)  opening flight, glide, follow, idle sway
 *                 controls.update      the user's pan / tilt / zoom, with easing
 *                 postFx.render        the 3D picture, with glow  (a <canvas>)
 *                 labels.render        the HTML labels  (a <div> on top of it)
 * ```
 *
 * Mouse: drag = move, right-drag = turn and tilt, wheel = zoom, click = select.
 */

import {
  ACESFilmicToneMapping,
  PCFShadowMap,
  PerspectiveCamera,
  Raycaster,
  Scene,
  Vector2,
  WebGLRenderer,
  type Material,
  type Mesh,
  type Object3D,
} from 'three'
import { MapControls } from 'three/addons/controls/MapControls.js'
import { CSS2DRenderer } from 'three/addons/renderers/CSS2DRenderer.js'

import { GATE, TOWN } from '../game/medinaLayout'
import { townBus } from '../game/townBus'
import { addAtmosphere } from './atmosphere'
import { createCameraDirector } from './cameraDirector'
import { clampToRect, distanceLimits, fitDistance, FOV_DEG, MAX_TURN, START_TILT, TILT_LIMITS } from './cameraRig'
import { rectCentre } from './coords'
import { createPostFx } from './postFx'
import { Town3d } from './Town3d'

/**
 * The HUD bar and the news strip float over the top and bottom of the canvas
 * (`--hud-height` and `--ticker-height` in `index.css`: keep in sync). The
 * first view fits the town between them.
 */
const HUD_INSETS = { top: 60, bottom: 36 } as const

/** A pointer that moved less than this between down and up was a click, not a drag (pixels). */
const CLICK_SLOP = 6

const TOWN_CENTRE = rectCentre(TOWN)
const TOWN_GROUND = { x: TOWN.x, z: TOWN.y, w: TOWN.w, d: TOWN.h }

export interface Town3dHandle {
  /** Stops the loop and frees everything. The caller must call it on unmount. */
  destroy(): void
}

/** Walks up from a hit mesh to the first object that knows what a click on it means. */
function clickHandler(hit: Object3D | null): (() => void) | undefined {
  for (let object = hit; object; object = object.parent) {
    const onClick: unknown = object.userData.onClick
    if (typeof onClick === 'function') return onClick as () => void
  }
  return undefined
}

export function createTown3d(parent: HTMLElement): Town3dHandle {
  // ---- renderer: the 3D canvas, and the HTML label layer over it ----
  // No `antialias` here: the picture is drawn off screen first, and `postFx`
  // smooths the edges there.
  const renderer = new WebGLRenderer()
  // Sharper on dense screens, but capped: 3× the pixels is 9× the work.
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
  renderer.shadowMap.enabled = true
  renderer.shadowMap.type = PCFShadowMap
  renderer.toneMapping = ACESFilmicToneMapping
  renderer.domElement.className = 'town3d-canvas'
  parent.append(renderer.domElement)

  const labels = new CSS2DRenderer()
  labels.domElement.className = 'town3d-labels'
  parent.append(labels.domElement)

  // ---- scene ----
  const scene = new Scene()
  addAtmosphere(scene)

  const town = new Town3d(scene)

  // ---- camera and controls ----
  const camera = new PerspectiveCamera(FOV_DEG, 1, 0.5, 600)
  const controls = new MapControls(camera, renderer.domElement)
  controls.enableDamping = true
  controls.screenSpacePanning = false // dragging moves over the ground, not up into the sky
  controls.zoomToCursor = true
  controls.minPolarAngle = TILT_LIMITS.min
  controls.maxPolarAngle = TILT_LIMITS.max
  controls.minAzimuthAngle = -MAX_TURN
  controls.maxAzimuthAngle = MAX_TURN

  // Both read the renderer's pixel ratio and tone mapping, set above.
  const postFx = createPostFx(renderer, scene, camera)
  const director = createCameraDirector({ camera, controls, town, element: parent, gate: rectCentre(GATE.tiles) })

  const onControlsChange = () => {
    // Keep the looked-at point over the town; move the camera by the same
    // amount so the view does not jump.
    const kept = clampToRect(controls.target.x, controls.target.z, TOWN_GROUND)
    const dx = kept.x - controls.target.x
    const dz = kept.z - controls.target.z
    if (dx === 0 && dz === 0) return
    controls.target.x = kept.x
    controls.target.z = kept.z
    camera.position.x += dx
    camera.position.z += dz
  }
  controls.addEventListener('change', onControlsChange)

  /**
   * Called at start and whenever the canvas changes size. It works out the
   * view that shows the whole town (the "home" view) and the zoom limits. The
   * director flies there the first time and keeps the user's view afterwards.
   */
  const fit = () => {
    const width = parent.clientWidth
    const height = parent.clientHeight
    if (width <= 0 || height <= 0) return // the panel is not laid out yet
    renderer.setSize(width, height)
    postFx.setSize(width, height)
    labels.setSize(width, height)
    camera.aspect = width / height
    camera.updateProjectionMatrix()

    const distance = fitDistance({ w: TOWN.w, d: TOWN.h }, { w: width, h: height }, HUD_INSETS, START_TILT)
    const limits = distanceLimits(distance)
    controls.minDistance = limits.min
    controls.maxDistance = limits.max

    // Looking a little south of the centre lifts the town on screen, away
    // from the taller HUD bar at the top.
    director.setHome(
      { target: { x: TOWN_CENTRE.x, z: TOWN_CENTRE.z + 1 }, distance, tilt: START_TILT, turn: 0 },
      limits,
    )
  }
  const resizeObserver = new ResizeObserver(fit)
  resizeObserver.observe(parent)
  fit()

  // ---- clicks ----
  const raycaster = new Raycaster()
  const pointer = new Vector2()
  let pressedAt: { x: number; y: number } | undefined

  /** What is under the pointer and clickable, if anything. */
  const pick = (event: PointerEvent) => {
    const box = renderer.domElement.getBoundingClientRect()
    // Screen pixels -> the -1..1 square three uses, y up.
    pointer.set(((event.clientX - box.left) / box.width) * 2 - 1, -((event.clientY - box.top) / box.height) * 2 + 1)
    raycaster.setFromCamera(pointer, camera)
    const hit = raycaster.intersectObjects(town.pickables, true)[0]
    return clickHandler(hit?.object ?? null)
  }
  const onPointerDown = (event: PointerEvent) => {
    if (event.button === 0) pressedAt = { x: event.clientX, y: event.clientY }
  }
  const onPointerUp = (event: PointerEvent) => {
    const start = pressedAt
    pressedAt = undefined
    if (!start || Math.hypot(event.clientX - start.x, event.clientY - start.y) > CLICK_SLOP) return
    pick(event)?.()
  }
  const onPointerMove = (event: PointerEvent) => {
    if (event.buttons !== 0) return // dragging: leave the cursor alone
    renderer.domElement.style.cursor = pick(event) ? 'pointer' : 'grab'
  }
  renderer.domElement.addEventListener('pointerdown', onPointerDown)
  renderer.domElement.addEventListener('pointerup', onPointerUp)
  renderer.domElement.addEventListener('pointermove', onPointerMove)

  // ---- frame loop ----
  let frame = 0
  let lastTime = performance.now()
  const loop = (time: number) => {
    frame = requestAnimationFrame(loop)
    // The browser stops frames in a hidden tab; cap the catch-up so a
    // character does not teleport through a whole walk in one step.
    const deltaSeconds = Math.min(1, (time - lastTime) / 1000)
    lastTime = time
    town.update(deltaSeconds)
    director.update(deltaSeconds)
    controls.update()
    postFx.render()
    labels.render(scene, camera)
  }
  frame = requestAnimationFrame(loop)

  // `TownCanvas` starts listening for "town:ready" right after this function
  // returns, so the event must not fire before then.
  let destroyed = false
  const readyTimer = window.setTimeout(() => {
    if (!destroyed) townBus.emit('town:ready')
  }, 0)

  return {
    destroy() {
      destroyed = true
      window.clearTimeout(readyTimer)
      cancelAnimationFrame(frame)
      resizeObserver.disconnect()
      director.dispose()
      controls.removeEventListener('change', onControlsChange)
      controls.dispose()
      town.dispose()

      // Free the graphics card memory. Shapes and materials are shared, so
      // collect them first and free each one once.
      const disposables = new Set<{ dispose(): void }>()
      scene.traverse((object) => {
        const mesh = object as Mesh
        if (!mesh.isMesh) return
        disposables.add(mesh.geometry)
        const meshMaterials: Material[] = Array.isArray(mesh.material) ? mesh.material : [mesh.material]
        for (const material of meshMaterials) {
          disposables.add(material)
          const map: unknown = (material as { map?: unknown }).map
          if (map && typeof (map as { dispose?: unknown }).dispose === 'function') {
            disposables.add(map as { dispose(): void })
          }
        }
      })
      for (const disposable of disposables) disposable.dispose()
      postFx.dispose()
      renderer.dispose()
      renderer.domElement.remove()
      labels.domElement.remove()
    },
  }
}
