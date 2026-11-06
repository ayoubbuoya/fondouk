/**
 * The town map (F8.1, F8.2, F2.3, F2.4): tile ground, village buildings with
 * live APY labels, and one walking sprite per character.
 *
 * ```text
 *        14.0%                         13.1%        top villages: labels above
 *    Rhea Lend USDC                Rhea Lend USDT
 *       ~~~~~~~~                      ~~~~~~~~
 *       [house]                        [house]
 *        ☺☺☺  ╲                      ╱  ☺          queue grows towards the arena
 *               ╲    ┌─Arena─┐    ╱
 *                    │ ☺ ☺   │                     all-cash characters
 *               ╱    └───────┘    ╲
 *        ☺☺  ╱                       ╲  ☺☺
 *       [house]                        [house]
 *       ~~~~~~~~ ...                               bottom villages: labels below
 * ```
 *
 * Data only arrives through `townBus` (React owns it, D48); the scene only
 * reports clicks back. Village positions are the DB `map_x/map_y` (0–400)
 * × `SCALE`. Art: Ninja Adventure, CC0 (`public/assets/CREDITS.md`).
 */

import { GameObjects, Scene, Textures, Tilemaps, Tweens } from 'phaser'

import type { AvatarSummary, Village, YieldPoint } from '../lib/api'
import { formatApy, staleLabel, totalApy } from '../lib/format'
import {
  directionOf,
  knownSprite,
  SPRITES,
  standFrame,
  walkAnimKey,
  walkFrames,
  walkSheetUrl,
  walkTextureKey,
  type Direction,
  type SpriteId,
} from './sprites'
import { townBus, type AvatarMoved, type VillageHistory } from './townBus'
import { buildTownTiles, MAP_PIXELS, pixelToTile, TILE_SCALE, TILE_SIZE } from './townMap'
import { CENTER, layoutStandings, walkPath, type Point } from './walkPath'

/** The database grid is 400×400 (migration 0002). */
const WORLD_SIZE = 400
const SCALE = MAP_PIXELS / WORLD_SIZE
export const CANVAS_SIZE = MAP_PIXELS

/** A house in `house.png` is 4 × 3 tiles; drawn ×2 = 128 × 96. */
const HOUSE_W = 64
const HOUSE_H = 48
const HOUSE_HALF_H = (HOUSE_H * TILE_SCALE) / 2
/** Where each house is in `house.png` (picked by eye, D53). */
const HOUSE_FRAMES = {
  lending: { x: 192, y: 0 }, // red tile roof
  pool: { x: 0, y: 0 }, // orange roof
}

const SPARK_W = 90
const SPARK_H = 26
/** Redraw so "stale · N min ago" stays true between hourly updates. */
const STALE_REFRESH_MS = 60_000

/** 200 px/s: village -> centre -> village (~570 px) takes under 3 s (F8 "within 5 s"). */
const WALK_SPEED_PX_PER_S = 200
const WALK_FPS = 8
const STALE_TINT = 0x8a8a8a

const TEXT_FONT = 'system-ui, sans-serif'

/** One character on the map. */
interface AvatarSprite {
  container: GameObjects.Container
  body: GameObjects.Sprite
  sprite: SpriteId
  /** Where it stands or is going; `null` = arena (all cash). */
  villageId: string | null
  /** The spot it is standing on, or walking to. */
  target: Point
  walk?: Tweens.TweenChain
}

export class TownScene extends Scene {
  private villages: Village[] = []
  private histories = new Map<string, YieldPoint[]>()
  /** Village id -> pixel position, used for the sprite layout. */
  private villagePoints = new Map<string, Point>()
  private avatars = new Map<string, AvatarSprite>()

  private ground?: Tilemaps.Tilemap
  /** The village positions the ground was built for; rebuilt only if they change. */
  private groundKey = ''
  /** Buildings and labels; rebuilt on each update (4 villages, cheap). */
  private villageLayer!: GameObjects.Container

  constructor() {
    super('town')
  }

  preload(): void {
    this.load.image('floor', '/assets/tiles/floor.png')
    this.load.image('house', '/assets/tiles/house.png')
    for (const sprite of SPRITES) {
      this.load.spritesheet(walkTextureKey(sprite), walkSheetUrl(sprite), {
        frameWidth: TILE_SIZE,
        frameHeight: TILE_SIZE,
      })
    }
  }

  create(): void {
    // Crisp pixels when drawn ×2, only for the art: a global `pixelArt`
    // setting would also make every text label blocky.
    for (const key of ['floor', 'house', ...SPRITES.map(walkTextureKey)]) {
      this.textures.get(key).setFilter(Textures.FilterMode.NEAREST)
    }
    const house = this.textures.get('house')
    for (const [name, frame] of Object.entries(HOUSE_FRAMES)) {
      house.add(name, 0, frame.x, frame.y, HOUSE_W, HOUSE_H)
    }
    this.createWalkAnimations()

    this.buildGround([])
    this.add
      .text(CENTER.x, CENTER.y - 58, 'Arena', {
        fontFamily: TEXT_FONT,
        fontSize: '16px',
        fontStyle: 'bold',
        color: '#ffffff',
        stroke: '#3b2f1a',
        strokeThickness: 4,
      })
      .setOrigin(0.5)
      .setDepth(1)
    this.villageLayer = this.add.container(0, 0).setDepth(1)

    const onVillages = (villages: Village[]) => {
      this.villages = villages
      this.villagePoints = new Map(
        villages.map((village) => [village.id, { x: village.map_x * SCALE, y: village.map_y * SCALE }]),
      )
      this.buildGround(villages)
      this.redrawVillages()
      this.relayoutAvatars()
    }
    const onHistory = ({ villageId, points }: VillageHistory) => {
      this.histories.set(villageId, points)
      this.redrawVillages()
    }
    const onAvatars = (avatars: AvatarSummary[]) => this.syncAvatars(avatars)
    const onMoved = ({ avatarId, to }: AvatarMoved) => {
      const avatar = this.avatars.get(avatarId)
      if (!avatar) return // React reloads `/avatars` for unknown ids.
      avatar.villageId = to
      this.relayoutAvatars()
    }
    townBus.on('villages:updated', onVillages)
    townBus.on('village:history', onHistory)
    townBus.on('avatars:updated', onAvatars)
    townBus.on('avatar:moved', onMoved)
    // Stop listening when the game is destroyed (React unmount), otherwise a
    // second game (StrictMode, hot reload) would get events for a dead scene.
    const unsubscribe = () => {
      townBus.off('villages:updated', onVillages)
      townBus.off('village:history', onHistory)
      townBus.off('avatars:updated', onAvatars)
      townBus.off('avatar:moved', onMoved)
    }
    this.events.once('shutdown', unsubscribe)
    this.events.once('destroy', unsubscribe)

    this.time.addEvent({ delay: STALE_REFRESH_MS, loop: true, callback: () => this.redrawVillages() })

    townBus.emit('town:ready')
  }

  /** One looping walk animation per character and direction (8 × 4). */
  private createWalkAnimations(): void {
    const directions: Direction[] = ['down', 'up', 'left', 'right']
    for (const sprite of SPRITES) {
      for (const direction of directions) {
        const key = walkAnimKey(sprite, direction)
        if (this.anims.exists(key)) continue // global: survives a scene restart
        this.anims.create({
          key,
          frames: this.anims.generateFrameNumbers(walkTextureKey(sprite), { frames: walkFrames(direction) }),
          frameRate: WALK_FPS,
          repeat: -1,
        })
      }
    }
  }

  // ---- ground ----

  /** (Re)builds the tile ground: grass, arena, and a road to each village. */
  private buildGround(villages: Village[]): void {
    const tiles = villages.map((village) => pixelToTile(village.map_x * SCALE, village.map_y * SCALE))
    const key = tiles.map((tile) => `${tile.tx},${tile.ty}`).join(';')
    if (this.ground && key === this.groundKey) return // live APY updates: nothing to redraw here
    this.groundKey = key

    this.ground?.destroy()
    this.ground = this.make.tilemap({ data: buildTownTiles(tiles), tileWidth: TILE_SIZE, tileHeight: TILE_SIZE })
    const tileset = this.ground.addTilesetImage('floor')
    if (!tileset) throw new Error('floor tileset did not load')
    this.ground.createLayer(0, tileset, 0, 0)?.setScale(TILE_SCALE).setDepth(0)
  }

  // ---- villages ----

  private redrawVillages(): void {
    this.villageLayer.removeAll(true)
    const now = new Date()
    for (const village of this.villages) {
      this.drawVillage(village, village.map_x * SCALE, village.map_y * SCALE, now)
    }
  }

  private drawVillage(village: Village, x: number, y: number, now: Date): void {
    const building = this.add
      .image(x, y, 'house', village.kind === 'lending' ? 'lending' : 'pool')
      .setScale(TILE_SCALE)
      .setInteractive({ useHandCursor: true })
      .on('pointerdown', () => townBus.emit('village:selected', village.id))
    if (village.stale) building.setTint(STALE_TINT)
    this.villageLayer.add(building)

    // Labels go on the side away from the arena, where no queue stands.
    const side = y < CENTER.y ? -1 : 1
    const edge = y + side * HOUSE_HALF_H
    const at = (distance: number) => edge + side * distance

    const sparkTop = side < 0 ? at(8) - SPARK_H : at(8)
    this.villageLayer.add(this.drawSparkline(this.histories.get(village.id) ?? [], x, sparkTop))
    this.villageLayer.add(
      this.add
        .text(x, at(SPARK_H + 22), village.name, {
          fontFamily: TEXT_FONT,
          fontSize: '15px',
          color: '#ffffff',
          stroke: '#1d1d1d',
          strokeThickness: 3,
        })
        .setOrigin(0.5),
    )
    this.villageLayer.add(
      this.add
        .text(x, at(SPARK_H + 48), formatApy(totalApy(village)), {
          fontFamily: TEXT_FONT,
          fontSize: '26px',
          fontStyle: 'bold',
          color: village.stale ? '#d0d0d0' : '#ffffff',
          stroke: '#1d1d1d',
          strokeThickness: 4,
        })
        .setOrigin(0.5),
    )

    const stale = staleLabel(village, now)
    if (stale) {
      this.villageLayer.add(
        this.add
          .text(x, at(SPARK_H + 76), stale, {
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

    spark.fillStyle(0x000000, 0.35)
    spark.fillRect(left - 3, top - 3, SPARK_W + 6, SPARK_H + 6)
    spark.lineStyle(2, 0xffffff, 1)
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

  // ---- characters ----

  /**
   * Matches the sprites to the latest `/avatars` list: new ones appear at
   * their spot, gone ones are removed, and a changed `main_village` (e.g.
   * after a reconnect) makes the sprite walk there.
   */
  private syncAvatars(avatars: AvatarSummary[]): void {
    const seen = new Set<string>()
    for (const avatar of avatars) {
      seen.add(avatar.id)
      const existing = this.avatars.get(avatar.id)
      if (existing) {
        existing.villageId = avatar.main_village
        continue
      }
      this.avatars.set(avatar.id, this.createAvatarSprite(avatar))
    }
    for (const [id, avatar] of this.avatars) {
      if (!seen.has(id)) {
        avatar.walk?.stop()
        avatar.container.destroy()
        this.avatars.delete(id)
      }
    }
    this.relayoutAvatars()
  }

  private createAvatarSprite(avatar: AvatarSummary): AvatarSprite {
    const sprite = knownSprite(avatar.sprite)
    const body = this.add.sprite(0, 0, walkTextureKey(sprite), standFrame('down')).setScale(TILE_SCALE)
    const name = this.add
      .text(0, -22, avatar.name, {
        fontFamily: TEXT_FONT,
        fontSize: '11px',
        color: '#ffffff',
        stroke: '#1d1d1d',
        strokeThickness: 3,
      })
      .setOrigin(0.5, 1)
    // New sprites start in the arena; the first layout puts them on their
    // spot without walking (see `relayoutAvatars`).
    const container = this.add.container(CENTER.x, CENTER.y, [body, name])
    container
      .setSize(TILE_SIZE * TILE_SCALE, TILE_SIZE * TILE_SCALE)
      .setInteractive({ useHandCursor: true })
      .on('pointerdown', () => townBus.emit('avatar:selected', avatar.id))
    return { container, body, sprite, villageId: avatar.main_village, target: { ...CENTER } }
  }

  /**
   * Gives every character its spot (`layoutStandings`) and walks the ones
   * whose spot changed. A sprite that was never placed jumps there instead.
   */
  private relayoutAvatars(): void {
    const standings = [...this.avatars].map(([id, avatar]) => ({ id, villageId: avatar.villageId }))
    const spots = layoutStandings(standings, this.villagePoints)

    for (const [id, avatar] of this.avatars) {
      const spot = spots.get(id)
      if (!spot || (spot.x === avatar.target.x && spot.y === avatar.target.y)) continue

      const placed = avatar.container.getData('placed') === true
      const previousVillage = avatar.container.getData('villageId') as string | null | undefined
      avatar.container.setData('villageId', avatar.villageId)
      avatar.target = spot
      if (!placed) {
        avatar.container.setPosition(spot.x, spot.y).setData('placed', true)
        this.updateDepth(avatar)
        continue
      }
      const from = { x: avatar.container.x, y: avatar.container.y }
      this.walk(avatar, walkPath(from, spot, previousVillage === avatar.villageId))
    }
  }

  /** Walks through `path`, facing and animating each step, then stands still. */
  private walk(avatar: AvatarSprite, path: Point[]): void {
    avatar.walk?.stop()
    if (path.length === 0) return

    let from = { x: avatar.container.x, y: avatar.container.y }
    let lastDirection: Direction = 'down'
    const steps = path.map((to) => {
      const start = from
      from = to
      const direction = directionOf(to.x - start.x, to.y - start.y)
      lastDirection = direction
      const duration = (Math.hypot(to.x - start.x, to.y - start.y) / WALK_SPEED_PX_PER_S) * 1000
      return {
        x: to.x,
        y: to.y,
        duration: Math.max(duration, 1),
        onStart: () => avatar.body.play(walkAnimKey(avatar.sprite, direction), true),
        onUpdate: () => this.updateDepth(avatar),
      }
    })
    avatar.walk = this.tweens.chain({
      targets: avatar.container,
      tweens: steps,
      onComplete: () => {
        avatar.body.stop()
        avatar.body.setFrame(standFrame(lastDirection))
      },
    })
  }

  /** Lower on screen = drawn in front, so queues overlap the right way. */
  private updateDepth(avatar: AvatarSprite): void {
    avatar.container.setDepth(2 + avatar.container.y / MAP_PIXELS)
  }
}
