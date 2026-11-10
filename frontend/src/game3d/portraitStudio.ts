/**
 * Renders the head-and-shoulders portrait of each 3D character, for the
 * panels (`face3d.png`, shown by `faceUrl` in the 3D view, D76).
 *
 * This is a **dev tool**, not part of the running app: the PNGs are made
 * once and committed, so the panels just load an image. To make them again
 * (after changing a model or a tint), open `/?view=3d` in the dev server and
 * run in the browser console:
 *
 * ```js
 * const studio = await import('/src/game3d/portraitStudio.ts')
 * const faces = await studio.renderAllPortraits()   // { boy: 'data:image/png;base64,...', ... }
 * ```
 *
 * then save each data URL as `public/assets/characters/<id>/face3d.png`.
 *
 * ```text
 *   camera ◉ ─────────────►  ☺   looks at the head mesh, from the front and a little above
 *                           ▐█▌  the frame is a bit taller than the head
 * ```
 */

import {
  Box3,
  DirectionalLight,
  HemisphereLight,
  Mesh,
  PerspectiveCamera,
  Scene,
  Vector3,
  WebGLRenderer,
  type Object3D,
} from 'three'

import { SPRITES, type SpriteId } from '../game/sprites'
import { CharacterBody } from './characters'

/** Pixels per side. Panels show faces at 20 to 52 px; 128 stays sharp on 2× screens. */
export const PORTRAIT_SIZE = 128

/** The frame is this many head-heights tall: a little room above and below the head. */
const HEAD_FRAME_FACTOR = 1.35
/** Without a head mesh: the frame covers this share of the model's height, from the top. */
const FRAME_SHARE = 0.5
const FOV_DEG = 30

/**
 * Renders every sprite id's portrait.
 *
 * @returns PNG data URLs (transparent background), by sprite id
 */
export async function renderAllPortraits(size = PORTRAIT_SIZE): Promise<Record<SpriteId, string>> {
  const renderer = new WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true })
  renderer.setPixelRatio(1)
  renderer.setSize(size, size)
  renderer.setClearColor(0x000000, 0) // transparent: the panel shows through

  const faces = {} as Record<SpriteId, string>
  try {
    for (const sprite of SPRITES) faces[sprite] = await renderPortrait(renderer, sprite)
  } finally {
    renderer.dispose()
  }
  return faces
}

/**
 * What the camera looks at, and how tall the frame is.
 *
 * Framed on the head mesh (`Knight_Head`, `Rogue_Head_Hooded`, ...), not on
 * the whole model: the mage's tall hat would otherwise fill the picture. The
 * frame is a bit taller than the head, so hoods and helmet tops show.
 * Without a head mesh (the stand-in figure), the top part of the body is used.
 */
function headFrame(body: CharacterBody): { target: Vector3; frame: number } {
  let head: Object3D | undefined
  body.root.traverse((child) => {
    if (!head && child instanceof Mesh && /_head/i.test(child.name)) head = child
  })
  if (head) {
    const box = new Box3().setFromObject(head)
    const centre = box.getCenter(new Vector3())
    return { target: new Vector3(0, centre.y, 0), frame: (box.max.y - box.min.y) * HEAD_FRAME_FACTOR }
  }
  const box = new Box3().setFromObject(body.root)
  const frame = (box.max.y - box.min.y) * FRAME_SHARE
  return { target: new Vector3(0, box.max.y - frame / 2, 0), frame }
}

async function renderPortrait(renderer: WebGLRenderer, sprite: SpriteId): Promise<string> {
  const scene = new Scene()
  // Same warm light as the town, with the sun in front so the face is lit.
  scene.add(new HemisphereLight(0xfff3dc, 0xc89460, 1.6))
  const sun = new DirectionalLight(0xffe0b0, 2.4)
  sun.position.set(-1.5, 3, 4)
  scene.add(sun)

  const body = new CharacterBody(sprite)
  scene.add(body.root)
  await body.ready
  // One small step so the idle pose is applied (otherwise: arms out, T-pose).
  body.update(0.1)
  body.root.updateMatrixWorld(true)

  const { target, frame } = headFrame(body)
  // Distance at which `frame` exactly fills the view's height.
  const distance = frame / 2 / Math.tan(((FOV_DEG / 2) * Math.PI) / 180)

  const camera = new PerspectiveCamera(FOV_DEG, 1, 0.01, 50)
  // Models face +z, so the camera stands on +z; a little higher than the
  // target, looking slightly down, like a passport photo taken by a tall person.
  camera.position.set(0, target.y + distance * 0.12, distance)
  camera.lookAt(target)

  renderer.render(scene, camera)
  const png = renderer.domElement.toDataURL('image/png')
  body.dispose()
  return png
}
