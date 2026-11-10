/**
 * Starts the 3D town inside `parent` (D76): the 3D twin of `game/createGame.ts`.
 *
 * This file is the "stage crew": the renderer, the lights, the camera and
 * its controls, the click detection and the frame loop. What is *on* the
 * stage is `Town3d`.
 *
 * ```text
 *   every frame:  town.update(dt)   walks and animations
 *                 camera move       a running "camera:focus" glide, if any
 *                 controls.update   the user's pan / tilt / zoom, with easing
 *                 renderer.render   the 3D picture   (a <canvas>)
 *                 labels.render     the HTML labels  (a <div> on top of it)
 * ```
 *
 * Mouse: drag = move, right-drag = turn and tilt, wheel = zoom, click = select.
 */

import {
  ACESFilmicToneMapping,
  Color,
  DirectionalLight,
  Fog,
  HemisphereLight,
  PCFShadowMap,
  PerspectiveCamera,
  Raycaster,
  Scene,
  Vector2,
  Vector3,
  WebGLRenderer,
  type Material,
  type Mesh,
  type Object3D,
} from 'three'
import { MapControls } from 'three/addons/controls/MapControls.js'
import { CSS2DRenderer } from 'three/addons/renderers/CSS2DRenderer.js'

import { TOWN } from '../game/medinaLayout'
import { townBus, type CameraFocus } from '../game/townBus'
import {
  clampToRect,
  distanceLimits,
  easeInOut,
  fitDistance,
  FOCUS_DISTANCE,
  FOV_DEG,
  START_TILT,
  TILT_LIMITS,
} from './cameraRig'
import { rectCentre } from './coords'
import { Town3d } from './Town3d'

/** The warm haze the desert fades into; also the sky colour. */
const SKY = 0xf2c58f

/**
 * The HUD bar and the news strip float over the top and bottom of the canvas
 * (`--hud-height` and `--ticker-height` in `index.css`: keep in sync, as in
 * `TownScene.ts`). The first view fits the town between them.
 */
const HUD_INSETS = { top: 60, bottom: 36 } as const

/** How far the user may turn the town, in radians: house fronts face south, so never show their backs. */
const MAX_TURN = 0.9
/** A pointer that moved less than this between down and up was a click, not a drag (pixels). */
const CLICK_SLOP = 6
const FOCUS_SECONDS = 0.6

const TOWN_CENTRE = rectCentre(TOWN)
const TOWN_GROUND = { x: TOWN.x, z: TOWN.y, w: TOWN.w, d: TOWN.h }

export interface Town3dHandle {
  /** Stops the loop and frees everything. The caller must call it on unmount. */
  destroy(): void
}

/** A camera glide from where it is to a new view. */
interface Glide {
  fromTarget: Vector3
  toTarget: Vector3
  /** Camera position minus target. */
  fromOffset: Vector3
  toOffset: Vector3
  elapsed: number
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
  const renderer = new WebGLRenderer({ antialias: true })
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

  // ---- scene and lights ----
  const scene = new Scene()
  scene.background = new Color(SKY)
  scene.fog = new Fog(SKY, 90, 260)
  // Soft light from the sky and bounced off the sand, so shadows are not black.
  scene.add(new HemisphereLight(0xfff3dc, 0xc89460, 1.3))
  // The sun: low and warm, from the south-west, so buildings throw long shadows.
  const sun = new DirectionalLight(0xffe0b0, 2.8)
  sun.position.set(TOWN_CENTRE.x - 30, 45, TOWN_CENTRE.z + 25)
  sun.target.position.set(TOWN_CENTRE.x, 0, TOWN_CENTRE.z)
  sun.castShadow = true
  sun.shadow.mapSize.set(2048, 2048)
  sun.shadow.camera.left = -48
  sun.shadow.camera.right = 48
  sun.shadow.camera.top = 40
  sun.shadow.camera.bottom = -40
  sun.shadow.camera.near = 1
  sun.shadow.camera.far = 160
  // Stops flat surfaces from shadowing themselves in stripes.
  sun.shadow.bias = -0.0005
  scene.add(sun, sun.target)

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

  /** The opening view, also where a `camera:focus` reset returns to. */
  const home = { target: new Vector3(), offset: new Vector3() }
  let cameraPlaced = false
  let glide: Glide | undefined

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
   * Called at start and whenever the canvas changes size. The first time it
   * shows the whole town; later it only updates the zoom limits, so the
   * user's view is kept.
   */
  const fit = () => {
    const width = parent.clientWidth
    const height = parent.clientHeight
    if (width <= 0 || height <= 0) return // the panel is not laid out yet
    renderer.setSize(width, height)
    labels.setSize(width, height)
    camera.aspect = width / height
    camera.updateProjectionMatrix()

    const distance = fitDistance({ w: TOWN.w, d: TOWN.h }, { w: width, h: height }, HUD_INSETS, START_TILT)
    const limits = distanceLimits(distance)
    controls.minDistance = limits.min
    controls.maxDistance = limits.max

    // Looking a little south of the centre lifts the town on screen, away
    // from the taller HUD bar at the top.
    home.target.set(TOWN_CENTRE.x, 0, TOWN_CENTRE.z + 1)
    home.offset.set(0, distance * Math.cos(START_TILT), distance * Math.sin(START_TILT))
    if (!cameraPlaced) {
      cameraPlaced = true
      controls.target.copy(home.target)
      camera.position.copy(home.target).add(home.offset)
      controls.update()
    }
  }
  const resizeObserver = new ResizeObserver(fit)
  resizeObserver.observe(parent)
  fit()

  /** Glides to a character, a village, a landmark or back home (used by the Judge Tour). */
  const onFocus = (focus: CameraFocus) => {
    const point = focus.reset ? null : town.focusPoint(focus)
    if (!focus.reset && !point) return // its target is not in the town
    const fromOffset = camera.position.clone().sub(controls.target)
    glide = {
      fromTarget: controls.target.clone(),
      fromOffset,
      toTarget: point ? new Vector3(point.x, 0, point.z) : home.target.clone(),
      // Same viewing angle, closer in; `CameraFocus.zoom` is a 2D camera
      // number with no meaning here, and no caller sets it.
      toOffset: point ? fromOffset.clone().setLength(FOCUS_DISTANCE) : home.offset.clone(),
      elapsed: 0,
    }
    controls.enabled = false // the user's drag must not fight the glide
  }
  townBus.on('camera:focus', onFocus)

  const stepGlide = (deltaSeconds: number) => {
    if (!glide) return
    glide.elapsed += deltaSeconds
    const progress = easeInOut(glide.elapsed / FOCUS_SECONDS)
    controls.target.lerpVectors(glide.fromTarget, glide.toTarget, progress)
    camera.position.lerpVectors(glide.fromOffset, glide.toOffset, progress).add(controls.target)
    if (glide.elapsed >= FOCUS_SECONDS) {
      glide = undefined
      controls.enabled = true
    }
  }

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
    stepGlide(deltaSeconds)
    controls.update()
    renderer.render(scene, camera)
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
      townBus.off('camera:focus', onFocus)
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
      renderer.dispose()
      renderer.domElement.remove()
      labels.domElement.remove()
    },
  }
}
