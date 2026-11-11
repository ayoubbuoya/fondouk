/**
 * The background life of the 3D town (D81): nothing here means data, it only
 * makes the town feel lived in.
 *
 * ```text
 *   buildings.ts labels parts       ambient.ts moves them, every frame
 *   userData.ambient = 'sway'     ─► palm crown rocks             (swayAngle)
 *                      'flutter'  ─► banner cloth swings          (swayAngle)
 *                      'smoke'    ─► grey puffs rise and fade     (smokePuff)
 *                      'fountain' ─► water drops jump and fall    (arcPoint)
 * ```
 *
 * The labelled parts are collected **once**, when this is created: the
 * decorations never change after the town is built, so nothing is searched
 * per frame. Visitors who asked for less motion get none of this.
 */

import { Mesh, MeshStandardMaterial, SphereGeometry, Vector3, type Object3D, type Scene } from 'three'

import type { AmbientKind } from './buildings'
import { arcPoint, smokePuff, swayAngle, type WorldPoint } from './motion'

/** Palm crowns: a small rock on two axes, each with its own pace. */
const PALM = { tilt: 0.07, period: 4.5, nod: 0.045, nodPeriod: 5.3 } as const
const BANNER = { swing: 0.35, period: 2.6 } as const
/** Light grey: a darker grey vanishes against the evening sand. */
const SMOKE = { puffs: 8, life: 5, color: 0xc4b8bc, drift: 0.35 } as const
/** `fall` = how far below the top of the fountain the water lands. */
const FOUNTAIN = { drops: 12, seconds: 1.1, radius: 0.72, fall: 1.67, hump: 0.45 } as const

const PUFF_SHAPE = new SphereGeometry(0.3, 10, 8)
// Larger than real drops, so they read from the normal camera distance.
const DROP_SHAPE = new SphereGeometry(0.09, 6, 5)
// A little light of their own so they stay blue in the evening, but below the
// bloom threshold, so they do not melt into the glow of the gold orb.
const DROP_MATERIAL = new MeshStandardMaterial({
  color: 0x9fd8f0,
  emissive: 0x4fb3d9,
  emissiveIntensity: 0.35,
  roughness: 0.2,
})

interface Turning {
  part: Object3D
  /** Out of step with its neighbours: taken from where it stands. */
  phase: number
}

interface Puff {
  mesh: Mesh
  material: MeshStandardMaterial
  base: Vector3
  age: number
}

interface Drop {
  mesh: Mesh
  top: WorldPoint
  /** Which drop around the circle it is (0..1). */
  slot: number
  age: number
}

/** A fixed phase from a position, so the town moves the same way on every load. */
function phaseOf(position: Vector3): number {
  return position.x * 0.7 + position.z * 1.3
}

export class Ambient {
  private readonly palms: Turning[] = []
  private readonly banners: Turning[] = []
  private readonly puffs: Puff[] = []
  private readonly drops: Drop[] = []
  private readonly enabled: boolean
  private seconds = 0

  /**
   * Collects the labelled parts already in `scene` and adds the smoke and
   * water around them. Create it after the decorations are built.
   */
  constructor(scene: Scene, enabled: boolean) {
    this.enabled = enabled
    if (!enabled) return

    const found: { part: Object3D; kind: AmbientKind }[] = []
    scene.traverse((object) => {
      const kind = object.userData.ambient as AmbientKind | undefined
      if (kind) found.push({ part: object, kind })
    })
    // Added after the walk: adding to the scene while `traverse` runs would visit the new puffs too.
    for (const { part, kind } of found) {
      const where = part.getWorldPosition(new Vector3())
      if (kind === 'sway') this.palms.push({ part, phase: phaseOf(where) })
      else if (kind === 'flutter') this.banners.push({ part, phase: phaseOf(where) })
      else if (kind === 'smoke') this.addSmoke(scene, where)
      else this.addFountain(scene, where)
    }
    this.update(0)
  }

  update(deltaSeconds: number): void {
    if (!this.enabled) return
    this.seconds += deltaSeconds
    const t = this.seconds

    for (const { part, phase } of this.palms) {
      part.rotation.z = swayAngle(t, phase, PALM.tilt, PALM.period)
      part.rotation.x = swayAngle(t, phase + 1.7, PALM.nod, PALM.nodPeriod)
    }
    for (const { part, phase } of this.banners) {
      part.rotation.y = swayAngle(t, phase, BANNER.swing, BANNER.period)
    }

    for (const puff of this.puffs) {
      // A puff that finished starts again at the chimney: it is invisible at
      // both ends of its life (`smokePuff`), so the restart never shows.
      puff.age = (puff.age + deltaSeconds) % SMOKE.life
      const look = smokePuff(puff.age, SMOKE.life)
      // A light wind pushes the smoke east as it rises.
      puff.mesh.position.set(puff.base.x + look.rise * SMOKE.drift, puff.base.y + look.rise, puff.base.z)
      puff.mesh.scale.setScalar(look.scale)
      puff.material.opacity = look.opacity
    }

    for (const drop of this.drops) {
      drop.age = (drop.age + deltaSeconds) % FOUNTAIN.seconds
      // The landing spots turn slowly round the basin, so the water never looks frozen.
      const angle = drop.slot * Math.PI * 2 + t * 0.3
      const landing = {
        x: drop.top.x + Math.cos(angle) * FOUNTAIN.radius,
        y: drop.top.y - FOUNTAIN.fall,
        z: drop.top.z + Math.sin(angle) * FOUNTAIN.radius,
      }
      const point = arcPoint(drop.top, landing, FOUNTAIN.hump, drop.age / FOUNTAIN.seconds)
      drop.mesh.position.set(point.x, point.y, point.z)
    }
  }

  /** Removes the puffs and drops and frees what only they use. */
  dispose(): void {
    for (const puff of this.puffs) {
      puff.mesh.removeFromParent()
      puff.material.dispose()
    }
    for (const drop of this.drops) drop.mesh.removeFromParent()
    this.puffs.length = 0
    this.drops.length = 0
  }

  private addSmoke(scene: Scene, base: Vector3): void {
    for (let i = 0; i < SMOKE.puffs; i++) {
      // Each puff fades on its own, so each needs its own material.
      const material = new MeshStandardMaterial({
        color: SMOKE.color,
        roughness: 1,
        transparent: true,
        opacity: 0,
        depthWrite: false,
      })
      const mesh = new Mesh(PUFF_SHAPE, material)
      scene.add(mesh)
      // Spread over one life, so there is always a column of smoke, not one cloud at a time.
      this.puffs.push({ mesh, material, base, age: (i / SMOKE.puffs) * SMOKE.life })
    }
  }

  private addFountain(scene: Scene, top: Vector3): void {
    for (let i = 0; i < FOUNTAIN.drops; i++) {
      const mesh = new Mesh(DROP_SHAPE, DROP_MATERIAL)
      scene.add(mesh)
      this.drops.push({
        mesh,
        top: { x: top.x, y: top.y, z: top.z },
        slot: i / FOUNTAIN.drops,
        // Spread over one jump, so water is always in the air.
        age: ((i * 5) % FOUNTAIN.drops) / FOUNTAIN.drops * FOUNTAIN.seconds,
      })
    }
  }
}
