/**
 * Effects that show the money story in the 3D town (D80). `Town3d` creates
 * them, feeds them events and calls `update` every frame.
 *
 * ```text
 *   "avatar:moved"      ─► CoinFlights.spawn(merchant, village)   coins fly first, the merchant walks after
 *   "villages:updated"  ─► YieldGlow.setRings(…)                  a gold ring per village, brighter = better APY
 * ```
 *
 * **No amounts, anywhere.** The `avatar_move` event carries none, and a number
 * here could mix `sim` and `real` money, which the project forbids. The coins
 * say "money moved here", the village sign says the yield.
 */

import {
  AdditiveBlending,
  Color,
  CylinderGeometry,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  RingGeometry,
  type Scene,
} from 'three'

import type { GroundPoint } from './coords'
import { arcPoint, type WorldPoint } from './motion'

const GOLD = 0xe8b45a

// ---- coins ----

const COINS_PER_MOVE = 6
/** Seconds one coin takes from the merchant to the village. */
const COIN_FLIGHT_SECONDS = 1.3
/** Seconds between two coins leaving. */
const COIN_GAP_SECONDS = 0.12
/** Height of the hump of the flight, in tiles. */
const COIN_ARC_HEIGHT = 3.5
/** Coins leave from about the merchant's hands and land at the door. */
const COIN_START_HEIGHT = 1
const COIN_END_HEIGHT = 0.8
/** Turns per second while flying. */
const COIN_SPIN = 2

// One shape and one material for every coin. It shines brighter than white,
// so the bloom in `postFx.ts` makes it glow.
// Larger than a real coin, like the characters: it must read from the whole-town view.
const COIN_SHAPE = new CylinderGeometry(0.34, 0.34, 0.08, 16)
const COIN_MATERIAL = new MeshStandardMaterial({
  color: GOLD,
  emissive: GOLD,
  emissiveIntensity: 1.8,
  metalness: 0.6,
  roughness: 0.4,
})

interface CoinFlight {
  mesh: Mesh
  from: WorldPoint
  to: WorldPoint
  /** Seconds since `spawn`; the coin waits `delay` before it leaves. */
  elapsed: number
  delay: number
}

/** The coins flying through the town right now. */
export class CoinFlights {
  private readonly scene: Scene
  private flights: CoinFlight[] = []

  constructor(scene: Scene) {
    this.scene = scene
  }

  /** Sends a handful of coins from `from` (the merchant) to `to` (a village door or the court). */
  spawn(from: GroundPoint, to: GroundPoint): void {
    for (let i = 0; i < COINS_PER_MOVE; i++) {
      const mesh = new Mesh(COIN_SHAPE, COIN_MATERIAL)
      mesh.rotation.x = Math.PI / 2 // stand the coin on its edge
      mesh.visible = false // until its turn to leave
      this.scene.add(mesh)
      this.flights.push({
        mesh,
        from: { x: from.x, y: COIN_START_HEIGHT, z: from.z },
        to: { x: to.x, y: COIN_END_HEIGHT, z: to.z },
        elapsed: 0,
        delay: i * COIN_GAP_SECONDS,
      })
    }
  }

  /** Moves every coin; a coin that landed is removed. */
  update(deltaSeconds: number): void {
    const flying: CoinFlight[] = []
    for (const flight of this.flights) {
      flight.elapsed += deltaSeconds
      const t = (flight.elapsed - flight.delay) / COIN_FLIGHT_SECONDS
      if (t >= 1) {
        flight.mesh.removeFromParent()
        continue
      }
      flying.push(flight)
      if (t < 0) continue // still waiting to leave
      const point = arcPoint(flight.from, flight.to, COIN_ARC_HEIGHT, t)
      flight.mesh.visible = true
      flight.mesh.position.set(point.x, point.y, point.z)
      flight.mesh.rotation.z = flight.elapsed * COIN_SPIN * Math.PI * 2
    }
    this.flights = flying
  }

  dispose(): void {
    for (const flight of this.flights) flight.mesh.removeFromParent()
    this.flights = []
  }
}

// ---- yield glow ----

/** One ring to draw: around which point, how big, how bright (0..1, from `apyGlowLevels`). */
export interface RingSpec {
  centre: GroundPoint
  /** Outer radius, in tiles. */
  radius: number
  level: number
}

const RING_WIDTH = 0.45
/** Just above the ground, so ring and sand never flicker into each other. */
const RING_HEIGHT = 0.03
/** The best village's ring is this bright at the top of a pulse. */
const RING_OPACITY = { base: 0.55, pulse: 0.2 } as const
const RING_PULSE_SECONDS = 3
/**
 * Brighter than white on purpose: added onto the sand it goes past the bloom
 * threshold, so the best ring glows a little (`postFx.ts`).
 */
const RING_COLOR = new Color(GOLD).multiplyScalar(1.6)

interface Ring {
  mesh: Mesh
  material: MeshBasicMaterial
  level: number
}

/** The gold rings on the ground around the villages. */
export class YieldGlow {
  private readonly scene: Scene
  private rings: Ring[] = []
  private seconds = 0

  constructor(scene: Scene) {
    this.scene = scene
  }

  /** Replaces all rings. Called with every village update (4 villages: cheap). */
  setRings(specs: RingSpec[]): void {
    this.clear()
    for (const spec of specs) {
      if (spec.level <= 0) continue // no yield (or stale): no ring at all
      // Each ring has its own material: its brightness is its own.
      const material = new MeshBasicMaterial({
        color: RING_COLOR,
        transparent: true,
        opacity: 0,
        depthWrite: false,
        blending: AdditiveBlending,
      })
      const mesh = new Mesh(new RingGeometry(spec.radius - RING_WIDTH, spec.radius, 48), material)
      mesh.rotation.x = -Math.PI / 2 // lay it flat on the ground
      mesh.position.set(spec.centre.x, RING_HEIGHT, spec.centre.z)
      this.scene.add(mesh)
      this.rings.push({ mesh, material, level: spec.level })
    }
    this.update(0)
  }

  /** Makes the rings breathe slowly, all together. */
  update(deltaSeconds: number): void {
    this.seconds += deltaSeconds
    const pulse = Math.sin((this.seconds / RING_PULSE_SECONDS) * Math.PI * 2)
    for (const ring of this.rings) {
      ring.material.opacity = ring.level * (RING_OPACITY.base + RING_OPACITY.pulse * pulse)
    }
  }

  dispose(): void {
    this.clear()
  }

  private clear(): void {
    for (const ring of this.rings) {
      ring.mesh.removeFromParent()
      ring.mesh.geometry.dispose()
      ring.material.dispose()
    }
    this.rings = []
  }
}
