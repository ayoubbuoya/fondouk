/**
 * The town map (F8.1, F8.2, F2.3, F2.4): the medina of Fondouk. Sand and
 * paved streets, a wall with a gate, one district per village with its live
 * APY sign, landmark buildings that open panels, and one walking sprite per
 * character.
 *
 * ```text
 *   draw order (depth)
 *   0      ground tiles (sand, paving)             built once
 *   2..3   buildings, props, characters            sorted by their feet:
 *                                                  lower on screen = in front
 *   5      signs (village APY, landmark names)     always on top, same size
 *                                                  on screen at any zoom
 * ```
 *
 * Where things are comes from `medinaLayout.ts` (HANDOFF D68); the DB
 * `map_x/map_y` are not read. Data only arrives through `townBus` (React
 * owns it, D48); the scene only reports clicks back.
 * Art: Ninja Adventure, CC0 (`public/assets/CREDITS.md`).
 */

import { GameObjects, Input, Scene, Structs, Textures, Tweens } from 'phaser'

import type { ArenaMessage, AvatarSummary, Village, YieldPoint } from '../lib/api'
import { formatApy, staleLabel, totalApy } from '../lib/format'
import { nextZoom, zoomLimits } from './cameraZoom'
import {
  assignDistricts,
  COURT,
  COURT_STATUE,
  DECOR_BUILDINGS,
  DECOR_PROPS,
  districtFrame,
  FRAMES,
  GATE,
  LANDMARKS,
  TILE_SCALE,
  TILE_SIZE,
  toPixels,
  TOWN,
  wallProps,
  WORLD_PIXELS,
  type District,
  type Place,
  type Prop,
} from './medinaLayout'
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
import { buildGroundTiles } from './townMap'
import { layoutStandings, pathLength, placeOf, walkPath, walkSpeed, type Point } from './walkPath'

const SPARK_W = 90
const SPARK_H = 26
/** Redraw so "stale · N min ago" stays true between hourly updates. */
const STALE_REFRESH_MS = 60_000

const WALK_FPS = 8
const STALE_TINT = 0x8a8a8a

const TEXT_FONT = 'system-ui, sans-serif'

/**
 * The HUD bar and the news strip float over the top and bottom of the canvas
 * (`--hud-height` and `--ticker-height` in `index.css`: keep in sync). The
 * first view fits the town between them, so no district sign starts hidden.
 */
const HUD_INSETS = { top: 60, bottom: 36 } as const

const DEPTH_GROUND = 0
/** Buildings, props and characters share this band, sorted by their feet. */
const DEPTH_WORLD = 2
const DEPTH_SIGNS = 5

/** One character on the map. */
interface AvatarSprite {
  container: GameObjects.Container
  body: GameObjects.Sprite
  /** The name tag; kept at the same size on screen at any zoom. */
  name: GameObjects.Text
  sprite: SpriteId
  /** Where it stands or is going; `null` = court (all cash). */
  villageId: string | null
  /** The place it stands in (or walks to); `undefined` until first placed. */
  place?: Place
  /** The spot it is standing on, or walking to. */
  target: Point
  walk?: Tweens.TweenChain
  /** Floating speech bubble shown when posting an arena message. */
  bubble?: GameObjects.Container
}

export class TownScene extends Scene {
  private villages: Village[] = []
  private histories = new Map<string, YieldPoint[]>()
  /** Village id -> its district; rebuilt when the village list changes. */
  private districts = new Map<string, District>()
  private avatars = new Map<string, AvatarSprite>()

  /** District buildings and their signs; rebuilt on each update (4 villages, cheap). */
  private villageObjects: GameObjects.GameObject[] = []
  /** Signs drawn at 1/zoom so they keep their size on screen. */
  private villageSigns: GameObjects.Container[] = []
  private landmarkSigns: GameObjects.Text[] = []

  private minZoom = 1
  /** `false` until the camera was placed once; later resizes keep the user's view. */
  private cameraPlaced = false

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
    for (const [name, [column, row, width, height]] of Object.entries(FRAMES)) {
      house.add(name, 0, column * TILE_SIZE, row * TILE_SIZE, width * TILE_SIZE, height * TILE_SIZE)
    }
    this.createWalkAnimations()

    this.buildGround()
    this.buildScenery()
    this.buildLandmarks()

    const onVillages = (villages: Village[]) => {
      this.villages = villages
      this.districts = assignDistricts(villages.map((village) => village.id))
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
    const onArenaMessage = (msg: ArenaMessage) => this.showSpeechBubble(msg)
    const onResize = (gameSize: Structs.Size) => this.fitCamera(gameSize.width, gameSize.height)

    townBus.on('villages:updated', onVillages)
    townBus.on('village:history', onHistory)
    townBus.on('avatars:updated', onAvatars)
    townBus.on('avatar:moved', onMoved)
    townBus.on('arena:message', onArenaMessage)
    this.scale.on('resize', onResize)
    // Stop listening when the game is destroyed (React unmount), otherwise a
    // second game (StrictMode, hot reload) would get events for a dead scene.
    const unsubscribe = () => {
      townBus.off('villages:updated', onVillages)
      townBus.off('village:history', onHistory)
      townBus.off('avatars:updated', onAvatars)
      townBus.off('avatar:moved', onMoved)
      townBus.off('arena:message', onArenaMessage)
      this.scale.off('resize', onResize)
    }
    this.events.once('shutdown', unsubscribe)
    this.events.once('destroy', unsubscribe)

    this.time.addEvent({ delay: STALE_REFRESH_MS, loop: true, callback: () => this.redrawVillages() })
    this.setUpCamera()

    townBus.emit('town:ready')
  }

  // ---- camera ----

  /**
   * Wheel zooms towards the cursor, dragging pans. The bounds stop the
   * camera at the world edge.
   */
  private setUpCamera(): void {
    const camera = this.cameras.main
    camera.setBounds(0, 0, WORLD_PIXELS.w, WORLD_PIXELS.h)
    this.fitCamera(this.scale.width, this.scale.height)

    this.input.on(
      'wheel',
      // Phaser already calls preventDefault on canvas wheel events
      // (`input.mouse.preventDefaultWheel` defaults to true), so the page does not scroll.
      (pointer: Input.Pointer, _over: GameObjects.GameObject[], _dx: number, deltaY: number) => {
        const zoom = nextZoom(camera.zoom, deltaY, this.minZoom)
        if (zoom === camera.zoom) return

        // Keep the world point under the cursor in place: remember it, zoom,
        // then scroll by however far it moved. `preRender` refreshes the
        // camera matrix that `getWorldPoint` reads.
        const before = camera.getWorldPoint(pointer.x, pointer.y)
        camera.setZoom(zoom)
        camera.preRender()
        const after = camera.getWorldPoint(pointer.x, pointer.y)
        camera.setScroll(camera.scrollX + before.x - after.x, camera.scrollY + before.y - after.y)
        this.applyLabelScale()
      },
    )

    this.input.on('pointermove', (pointer: Input.Pointer) => {
      if (!pointer.isDown) return
      // Pointer moves are in screen pixels; divide by zoom to get world pixels.
      camera.setScroll(
        camera.scrollX - (pointer.x - pointer.prevPosition.x) / camera.zoom,
        camera.scrollY - (pointer.y - pointer.prevPosition.y) / camera.zoom,
      )
    })
  }

  /**
   * Called at start and whenever the canvas changes size (window resize, a
   * side panel opening). The first time it shows the whole town; later it
   * only makes sure the zoom is still allowed, so the user's view is kept.
   */
  private fitCamera(width: number, height: number): void {
    if (width <= 0 || height <= 0) return // the panel is not laid out yet
    const camera = this.cameras.main
    const town = toPixels(TOWN)
    const limits = zoomLimits({ w: width, h: height }, WORLD_PIXELS, { w: town.w, h: town.h })
    this.minZoom = limits.min
    // The zoom that shows the whole town in the part of the canvas no bar covers.
    const freeHeight = Math.max(1, height - HUD_INSETS.top - HUD_INSETS.bottom)
    const start = zoomLimits({ w: width, h: freeHeight }, WORLD_PIXELS, { w: town.w, h: town.h }).start

    camera.setSize(width, height)
    if (!this.cameraPlaced) {
      this.cameraPlaced = true
      camera.setZoom(Math.max(limits.min, start))
      // Centre the town in the free part: shift by half the difference of the bars.
      const shift = (HUD_INSETS.top - HUD_INSETS.bottom) / 2 / camera.zoom
      camera.centerOn(town.x + town.w / 2, town.y + town.h / 2 - shift)
    } else if (camera.zoom < limits.min) {
      camera.setZoom(limits.min)
    }
    this.applyLabelScale()
  }

  /**
   * Zooming scales everything, so signs and name tags would shrink or
   * overlap. Drawing them at 1/zoom keeps them the same size on screen (and
   * sharp: their net scale is 1).
   */
  private applyLabelScale(): void {
    const scale = 1 / this.cameras.main.zoom
    for (const avatar of this.avatars.values()) {
      avatar.name.setScale(scale)
      avatar.bubble?.setScale(scale)
    }
    for (const sign of this.villageSigns) sign.setScale(scale)
    for (const sign of this.landmarkSigns) sign.setScale(scale)
  }

  /**
   * Shows a floating speech bubble above the talking avatar for 5 seconds (SPEC F9.1).
   */
  private showSpeechBubble(msg: ArenaMessage): void {
    const avatar = this.avatars.get(msg.avatar_id)
    if (!avatar) return

    avatar.bubble?.destroy()

    const bubbleContainer = this.add.container(0, -36)
    const maxTextLen = 45
    const displayText = msg.text.length > maxTextLen ? msg.text.slice(0, maxTextLen) + '…' : msg.text

    const textObj = this.add
      .text(0, -6, displayText, {
        fontFamily: TEXT_FONT,
        fontSize: '11px',
        color: '#ffffff',
        wordWrap: { width: 130 },
        align: 'center',
      })
      .setOrigin(0.5, 1)

    const bounds = textObj.getBounds()
    const padX = 8
    const padY = 5
    const w = Math.max(bounds.width + padX * 2, 44)
    const h = bounds.height + padY * 2

    const bg = this.add.graphics()
    bg.fillStyle(0x18181b, 0.92)
    bg.lineStyle(1.5, 0x60a5fa, 1)
    bg.fillRoundedRect(-w / 2, -h - 4, w, h, 6)
    bg.strokeRoundedRect(-w / 2, -h - 4, w, h, 6)
    // Little pointer triangle at the bottom pointing to the head
    bg.fillTriangle(-4, -4, 4, -4, 0, 0)

    textObj.setPosition(0, -padY - 4)
    bubbleContainer.add([bg, textObj])
    bubbleContainer.setDepth(15)
    bubbleContainer.setScale(1 / this.cameras.main.zoom)

    avatar.container.add(bubbleContainer)
    avatar.bubble = bubbleContainer

    this.tweens.add({
      targets: bubbleContainer,
      alpha: { from: 0, to: 1 },
      duration: 200,
      onComplete: () => {
        this.time.delayedCall(5000, () => {
          if (avatar.bubble === bubbleContainer) {
            this.tweens.add({
              targets: bubbleContainer,
              alpha: 0,
              duration: 400,
              onComplete: () => {
                bubbleContainer.destroy()
                if (avatar.bubble === bubbleContainer) avatar.bubble = undefined
              },
            })
          }
        })
      },
    })
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

  // ---- ground and scenery (built once: the layout never changes at runtime) ----

  private buildGround(): void {
    const ground = this.make.tilemap({ data: buildGroundTiles(), tileWidth: TILE_SIZE, tileHeight: TILE_SIZE })
    const tileset = ground.addTilesetImage('floor')
    if (!tileset) throw new Error('floor tileset did not load')
    ground.createLayer(0, tileset, 0, 0)?.setScale(TILE_SCALE).setDepth(DEPTH_GROUND)
  }

  /** Draws one piece of `house.png` where the layout says, sorted by its bottom edge. */
  private addProp(prop: Prop): GameObjects.Image {
    const box = toPixels(prop.tiles)
    return this.add
      .image(box.x, box.y, 'house', prop.frame)
      .setOrigin(0, 0)
      .setScale(TILE_SCALE)
      .setDepth(this.depthAt(box.y + box.h))
  }

  /** Lower on screen = drawn in front, so characters pass behind roofs and in front of doors. */
  private depthAt(feetY: number): number {
    return DEPTH_WORLD + feetY / WORLD_PIXELS.h
  }

  private buildScenery(): void {
    for (const prop of [...wallProps(), GATE, ...DECOR_BUILDINGS, ...DECOR_PROPS]) this.addProp(prop)
  }

  /** The court statue (opens the Arena) and the landmark buildings (open their panel). */
  private buildLandmarks(): void {
    const openArena = () => townBus.emit('arena:selected')
    const statue = toPixels(COURT_STATUE.tiles)
    this.addProp(COURT_STATUE).setInteractive({ useHandCursor: true }).on('pointerdown', openArena)
    this.landmarkSigns.push(this.addSign(statue.x + statue.w / 2, statue.y - 4, '💬 Arena', openArena))

    for (const landmark of LANDMARKS) {
      const open = () => townBus.emit('landmark:selected', landmark.id)
      const box = toPixels(landmark.prop.tiles)
      this.addProp(landmark.prop).setInteractive({ useHandCursor: true }).on('pointerdown', open)
      this.landmarkSigns.push(this.addSign(box.x + box.w / 2, box.y - 4, landmark.label, open))
    }
  }

  /** A clickable name above a landmark, anchored at its bottom centre. */
  private addSign(x: number, y: number, label: string, onClick: () => void): GameObjects.Text {
    return this.add
      .text(x, y, label, {
        fontFamily: TEXT_FONT,
        fontSize: '15px',
        fontStyle: 'bold',
        color: '#ffffff',
        stroke: '#3b2f1a',
        strokeThickness: 4,
      })
      .setOrigin(0.5, 1)
      .setDepth(DEPTH_SIGNS)
      .setInteractive({ useHandCursor: true })
      .on('pointerdown', onClick)
  }

  // ---- villages ----

  private redrawVillages(): void {
    for (const object of this.villageObjects) object.destroy()
    this.villageObjects = []
    this.villageSigns = []

    const now = new Date()
    for (const village of this.villages) {
      const district = this.districts.get(village.id)
      if (district) this.drawVillage(village, district, now)
    }
    this.applyLabelScale()
  }

  private drawVillage(village: Village, district: District, now: Date): void {
    const select = () => townBus.emit('village:selected', village.id)
    const building = this.addProp({ frame: districtFrame(village.kind), tiles: district.building })
      .setInteractive({ useHandCursor: true })
      .on('pointerdown', select)
    if (village.stale) building.setTint(STALE_TINT)
    this.villageObjects.push(building)

    // The sign sits above the roof and grows upwards from y = 0.
    const box = toPixels(district.building)
    const sign = this.add.container(box.x + box.w / 2, box.y - 2).setDepth(DEPTH_SIGNS)
    sign.add(this.drawSparkline(this.histories.get(village.id) ?? [], 0, -SPARK_H - 6))
    sign.add(
      this.add
        .text(0, -SPARK_H - 22, village.name, {
          fontFamily: TEXT_FONT,
          fontSize: '15px',
          color: '#ffffff',
          stroke: '#1d1d1d',
          strokeThickness: 3,
        })
        .setOrigin(0.5),
    )
    sign.add(
      this.add
        .text(0, -SPARK_H - 48, formatApy(totalApy(village)), {
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
      sign.add(
        this.add
          .text(0, -SPARK_H - 76, stale, {
            fontFamily: TEXT_FONT,
            fontSize: '13px',
            color: '#ffe08a',
            backgroundColor: '#3a3a3acc',
            padding: { x: 6, y: 2 },
          })
          .setOrigin(0.5),
      )
    }
    this.villageObjects.push(sign)
    this.villageSigns.push(sign)
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
      .setScale(1 / this.cameras.main.zoom) // match the others if added while zoomed
    // New sprites start in the court; the first layout puts them on their
    // spot without walking (see `relayoutAvatars`).
    const start = COURT.slotsOrigin
    const container = this.add.container(start.x, start.y, [body, name])
    container
      .setSize(TILE_SIZE * TILE_SCALE, TILE_SIZE * TILE_SCALE)
      .setInteractive({ useHandCursor: true })
      .on('pointerdown', () => townBus.emit('avatar:selected', avatar.id))
    return { container, body, name, sprite, villageId: avatar.main_village, target: { ...start } }
  }

  /**
   * Gives every character its spot (`layoutStandings`) and walks the ones
   * whose spot changed. A sprite that was never placed jumps there instead.
   */
  private relayoutAvatars(): void {
    const standings = [...this.avatars].map(([id, avatar]) => ({ id, villageId: avatar.villageId }))
    const spots = layoutStandings(standings, this.districts)

    for (const [id, avatar] of this.avatars) {
      const spot = spots.get(id)
      if (!spot || (spot.x === avatar.target.x && spot.y === avatar.target.y)) continue

      const previousPlace = avatar.place
      const place = placeOf(avatar.villageId, this.districts)
      avatar.place = place
      avatar.target = spot
      if (!previousPlace) {
        avatar.container.setPosition(spot.x, spot.y)
        this.updateDepth(avatar)
        continue
      }
      const from = { x: avatar.container.x, y: avatar.container.y }
      this.walk(avatar, walkPath(from, spot, previousPlace, place))
    }
  }

  /** Walks through `path`, facing and animating each step, then stands still. */
  private walk(avatar: AvatarSprite, path: Point[]): void {
    avatar.walk?.stop()
    if (path.length === 0) return

    let from = { x: avatar.container.x, y: avatar.container.y }
    const speed = walkSpeed(pathLength(from, path))
    let lastDirection: Direction = 'down'
    const steps = path.map((to) => {
      const start = from
      from = to
      const direction = directionOf(to.x - start.x, to.y - start.y)
      lastDirection = direction
      const duration = (Math.hypot(to.x - start.x, to.y - start.y) / speed) * 1000
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

  /** Sorted by the feet (the sprite's bottom edge), like buildings and props. */
  private updateDepth(avatar: AvatarSprite): void {
    avatar.container.setDepth(this.depthAt(avatar.container.y + (TILE_SIZE * TILE_SCALE) / 2))
  }
}
