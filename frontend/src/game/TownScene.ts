/**
 * The town map (F8.1, F2.3, F2.4), drawn in code until the real tileset
 * arrives on Day 7 (HANDOFF D45).
 *
 * ```text
 *        lend USDC ─────────────── lend USDT        each village:
 *            \                       /                 14.0%        <- APY label
 *             \       ┌───────┐     /                Rhea Lend USDC <- name
 *              ───────│ Arena │─────                   [building]   <- colour = kind
 *             /       └───────┘     \                  ~~~~~~~~     <- 7-day sparkline
 *            /                       \                 stale · 3 h  <- only if stale
 *      stable pool ────────────── FRAX-USDC
 * ```
 *
 * Positions come from the database (`villages.map_x/map_y`, a 0–400 grid),
 * multiplied by `SCALE` so text is drawn at full resolution.
 */

import { GameObjects, Scene } from 'phaser'

import type { Village, YieldPoint } from '../lib/api'
import { formatApy, staleLabel, totalApy } from '../lib/format'
import { townBus, type VillageHistory } from './townBus'

/** The database grid is 400×400 (migration 0002). */
const WORLD_SIZE = 400
const SCALE = 2
export const CANVAS_SIZE = WORLD_SIZE * SCALE
const CENTER = CANVAS_SIZE / 2

const ARENA_SIZE = 120
const BUILDING_W = 76
const BUILDING_H = 52
const SPARK_W = 90
const SPARK_H = 26
/** Redraw so "stale · N min ago" stays true between hourly updates. */
const STALE_REFRESH_MS = 60_000

const COLORS = {
  road: 0xd9c38c,
  arena: 0xc9a66b,
  lending: 0x4a7bd0,
  pool: 0x3fa37a,
  stale: 0x8a8a8a,
  outline: 0x2b2b2b,
  spark: 0xffffff,
}

const TEXT_FONT = 'system-ui, sans-serif'

export class TownScene extends Scene {
  private villages: Village[] = []
  private histories = new Map<string, YieldPoint[]>()
  /** Everything drawn for the villages; rebuilt on each update (4 villages, cheap). */
  private villageLayer!: GameObjects.Container
  private roads!: GameObjects.Graphics

  constructor() {
    super('town')
  }

  create(): void {
    this.roads = this.add.graphics()
    this.drawArena()
    this.villageLayer = this.add.container(0, 0)

    const onVillages = (villages: Village[]) => {
      this.villages = villages
      this.redraw()
    }
    const onHistory = ({ villageId, points }: VillageHistory) => {
      this.histories.set(villageId, points)
      this.redraw()
    }
    townBus.on('villages:updated', onVillages)
    townBus.on('village:history', onHistory)
    // Stop listening when the game is destroyed (React unmount), otherwise a
    // second game (StrictMode, hot reload) would get events for a dead scene.
    const unsubscribe = () => {
      townBus.off('villages:updated', onVillages)
      townBus.off('village:history', onHistory)
    }
    this.events.once('shutdown', unsubscribe)
    this.events.once('destroy', unsubscribe)

    this.time.addEvent({ delay: STALE_REFRESH_MS, loop: true, callback: () => this.redraw() })

    townBus.emit('town:ready')
  }

  private drawArena(): void {
    const arena = this.add.graphics()
    arena.fillStyle(COLORS.arena, 1)
    arena.fillRect(CENTER - ARENA_SIZE / 2, CENTER - ARENA_SIZE / 2, ARENA_SIZE, ARENA_SIZE)
    arena.lineStyle(3, COLORS.outline, 0.6)
    arena.strokeRect(CENTER - ARENA_SIZE / 2, CENTER - ARENA_SIZE / 2, ARENA_SIZE, ARENA_SIZE)
    this.add
      .text(CENTER, CENTER, 'Arena', { fontFamily: TEXT_FONT, fontSize: '18px', color: '#3b2f1a' })
      .setOrigin(0.5)
  }

  private redraw(): void {
    this.villageLayer.removeAll(true)
    this.roads.clear()
    this.roads.lineStyle(14, COLORS.road, 1)

    const now = new Date()
    for (const village of this.villages) {
      const x = village.map_x * SCALE
      const y = village.map_y * SCALE
      this.roads.lineBetween(CENTER, CENTER, x, y)
      this.drawVillage(village, x, y, now)
    }
  }

  private drawVillage(village: Village, x: number, y: number, now: Date): void {
    const color = village.stale ? COLORS.stale : village.kind === 'lending' ? COLORS.lending : COLORS.pool

    const building = this.add.graphics()
    building.fillStyle(color, 1)
    building.fillRoundedRect(x - BUILDING_W / 2, y - BUILDING_H / 2, BUILDING_W, BUILDING_H, 8)
    building.lineStyle(3, COLORS.outline, 0.8)
    building.strokeRoundedRect(x - BUILDING_W / 2, y - BUILDING_H / 2, BUILDING_W, BUILDING_H, 8)

    const apy = this.add
      .text(x, y - 72, formatApy(totalApy(village)), {
        fontFamily: TEXT_FONT,
        fontSize: '26px',
        fontStyle: 'bold',
        color: village.stale ? '#d0d0d0' : '#ffffff',
        stroke: '#1d1d1d',
        strokeThickness: 4,
      })
      .setOrigin(0.5)

    const name = this.add
      .text(x, y - 44, village.name, {
        fontFamily: TEXT_FONT,
        fontSize: '15px',
        color: '#ffffff',
        stroke: '#1d1d1d',
        strokeThickness: 3,
      })
      .setOrigin(0.5)

    const spark = this.drawSparkline(this.histories.get(village.id) ?? [], x, y + 34)
    this.villageLayer.add([building, apy, name, spark])

    const stale = staleLabel(village, now)
    if (stale) {
      this.villageLayer.add(
        this.add
          .text(x, y + 76, stale, {
            fontFamily: TEXT_FONT,
            fontSize: '13px',
            color: '#ffe08a',
            backgroundColor: '#3a3a3acc',
            padding: { x: 6, y: 2 },
          })
          .setOrigin(0.5),
      )
    }
  }

  /**
   * F2.3: a 7-day line of total APY, scaled to its own min/max so small moves
   * are visible. The box is centred on (cx, top). Fewer than 2 points: no line.
   */
  private drawSparkline(points: YieldPoint[], cx: number, top: number): GameObjects.Graphics {
    const spark = this.add.graphics()
    const values = points.map((point) =>
      totalApy({ apy_base: point.apy_base, apy_reward: point.apy_reward }) ?? 0,
    )
    if (values.length < 2) return spark

    const min = Math.min(...values)
    const max = Math.max(...values)
    const range = max - min
    const left = cx - SPARK_W / 2

    spark.fillStyle(0x000000, 0.25)
    spark.fillRect(left - 3, top - 3, SPARK_W + 6, SPARK_H + 6)
    spark.lineStyle(2, COLORS.spark, 1)
    spark.beginPath()
    values.forEach((value, index) => {
      const px = left + (index / (values.length - 1)) * SPARK_W
      // Flat history (range 0): draw a line through the middle.
      const ratio = range === 0 ? 0.5 : (value - min) / range
      const py = top + SPARK_H - ratio * SPARK_H
      if (index === 0) spark.moveTo(px, py)
      else spark.lineTo(px, py)
    })
    spark.strokePath()
    return spark
  }
}
