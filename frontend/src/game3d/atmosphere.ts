/**
 * The evening mood of the 3D town (D78): what lights it and what colour the
 * distance fades into. `createTown3d.ts` calls this once.
 *
 * ```text
 *   sky light (purple-blue)        fills the shadows, so they are never black
 *   sun (orange, low, south-west)  the last daylight; the only light that casts shadows
 *   2 lamps (court, gate)          warm pools of light on the ground, no shadows
 *   fog + background               far sand fades into the evening haze
 * ```
 *
 * The small lights you *see* (windows, lanterns) are not lights at all: they
 * are glowing materials in `buildings.ts`, made to shine by `postFx.ts`.
 * A real light per window would cost far too much to draw.
 */

import { Color, DirectionalLight, Fog, HemisphereLight, PointLight, type Scene } from 'three'

import { COURT_STATUE, GATE, TOWN } from '../game/medinaLayout'
import { rectCentre } from './coords'

/** The evening haze the desert fades into; also the colour behind everything. */
const HAZE = 0x6f4a6e

const SKY_LIGHT = { sky: 0xa99be0, ground: 0x9a6248, intensity: 1.15 } as const
const SUN = { color: 0xff9a55, intensity: 2.6 } as const
/** A lamp reaches `distance` tiles; past that it adds nothing, which keeps it cheap. */
const LAMP = { color: 0xffb060, intensity: 26, distance: 13 } as const

/** A warm lamp over a ground point. It casts no shadows: shadows are the costly part of a light. */
function lamp(at: { x: number; z: number }, height: number): PointLight {
  const light = new PointLight(LAMP.color, LAMP.intensity, LAMP.distance)
  light.position.set(at.x, height, at.z)
  return light
}

/** Adds the evening lights, the haze and the background colour to `scene`. */
export function addAtmosphere(scene: Scene): void {
  const centre = rectCentre(TOWN)

  scene.background = new Color(HAZE)
  scene.fog = new Fog(HAZE, 80, 240)
  scene.add(new HemisphereLight(SKY_LIGHT.sky, SKY_LIGHT.ground, SKY_LIGHT.intensity))

  // Low in the south-west: house fronts (which face south) catch the light
  // and every building throws a long shadow to the north-east.
  const sun = new DirectionalLight(SUN.color, SUN.intensity)
  sun.position.set(centre.x - 44, 24, centre.z + 26)
  sun.target.position.set(centre.x, 0, centre.z)
  sun.castShadow = true
  sun.shadow.mapSize.set(2048, 2048)
  // The box of ground the shadow picture covers, seen from the sun.
  sun.shadow.camera.left = -48
  sun.shadow.camera.right = 48
  sun.shadow.camera.top = 40
  sun.shadow.camera.bottom = -40
  sun.shadow.camera.near = 1
  sun.shadow.camera.far = 160
  // Stops flat surfaces from shadowing themselves in stripes.
  sun.shadow.bias = -0.0005
  scene.add(sun, sun.target)

  scene.add(lamp(rectCentre(COURT_STATUE.tiles), 2.8), lamp(rectCentre(GATE.tiles), 3))
}
