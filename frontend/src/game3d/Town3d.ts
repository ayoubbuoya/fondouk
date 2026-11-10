/**
 * What is in the 3D town and how it reacts to data (D76): the 3D twin of
 * `game/TownScene.ts`. Same `townBus` events in, same click events out, so
 * React cannot tell the two apart.
 *
 * ```text
 *   townBus                         Town3d
 *   "villages:updated"  ─────────►  district buildings + signs
 *   "village:history"   ─────────►  the line on a sign
 *   "avatars:updated"   ─────────►  add / remove / re-place characters
 *   "avatar:moved"      ─────────►  walk along the streets (walkPath -> walker)
 *   "arena:message"     ─────────►  speech bubble
 *   "avatar:milestone"  ─────────►  new level on the tag + floating text
 *   "demo:spawn_clone"  ─────────►  one extra character (Judge Tour)
 *
 *   click a building / sign / character  ─►  "village:selected", "avatar:selected",
 *                                            "arena:selected", "landmark:selected"
 * ```
 *
 * Positions come from `medinaLayout.ts` and `walkPath.ts`, in layout pixels;
 * `coords.ts` turns them into 3D. This class owns no camera and no renderer:
 * see `createTown3d.ts`.
 */

import { Box3, CylinderGeometry, Mesh, MeshBasicMaterial, type Object3D, type Scene } from 'three'

import {
  assignDistricts,
  COURT,
  COURT_STATUE,
  DECOR_BUILDINGS,
  DECOR_PROPS,
  districtFrame,
  GATE,
  LANDMARKS,
  wallProps,
  type District,
  type Place,
  type Point,
  type TileRect,
} from '../game/medinaLayout'
import { knownSprite } from '../game/sprites'
import { townBus, type AvatarMilestone, type AvatarMoved, type CameraFocus, type VillageHistory } from '../game/townBus'
import { layoutStandings, layoutTagTiers, pathLength, placeOf, walkPath, walkSpeed } from '../game/walkPath'
import type { ArenaMessage, AvatarSummary, Village, YieldPoint } from '../lib/api'
import { nameTag } from '../lib/progression'
import { buildFrame, buildGround, buildProp } from './buildings'
import { CHARACTER_HEIGHT, CharacterBody } from './characters'
import { facingAngle, rectCentre, toGround, type GroundPoint } from './coords'
import { AvatarLabel, createLandmarkSign, VillageSign } from './labels'
import { walkAt } from './walker'

/** Redraw so "stale · N min ago" stays true between hourly updates. */
const STALE_REFRESH_MS = 60_000

/** How far above a roof (or a head) a label is anchored, in tiles. */
const LABEL_GAP = 0.35

/**
 * Characters are clicked through this invisible cylinder, not their animated
 * mesh: it is cheaper to test and easier to hit.
 */
const PICK_SHAPE = new CylinderGeometry(0.5, 0.5, CHARACTER_HEIGHT * 1.1, 8)
const PICK_MATERIAL = new MeshBasicMaterial({ visible: false })

interface Walk {
  from: Point
  path: Point[]
  /** Layout pixels per second. */
  speed: number
  elapsed: number
}

/** One character in the town. Positions are layout pixels, like `walkPath.ts`. */
interface Avatar3d {
  body: CharacterBody
  label: AvatarLabel
  /** The name alone, to rebuild the tag when the level changes. */
  displayName: string
  /** Where it stands or is going; `null` = the court (all cash). */
  villageId: string | null
  /** The place it stands in (or walks to); `undefined` until first placed. */
  place?: Place
  /** The spot it stands on, or walks to. */
  target: Point
  /** Where it is right now. */
  position: Point
  walk?: Walk
}

/** The highest point of something built, to hang a label above it. */
function topOf(object: Object3D): number {
  return new Box3().setFromObject(object).max.y
}

export class Town3d {
  /** What a click can hit. `userData.onClick` on a hit object (or a parent) runs. */
  readonly pickables: Object3D[] = []

  private villages: Village[] = []
  private histories = new Map<string, YieldPoint[]>()
  private districts = new Map<string, District>()
  private avatars = new Map<string, Avatar3d>()

  /** District buildings, rebuilt when the village list changes (4 villages, cheap). */
  private villageBuildings: Object3D[] = []
  private villageSigns = new Map<string, VillageSign>()

  private readonly scene: Scene
  private readonly unsubscribe: () => void
  private readonly staleTimer: number

  constructor(scene: Scene) {
    this.scene = scene
    scene.add(buildGround())
    for (const prop of [...wallProps(), GATE, ...DECOR_BUILDINGS, ...DECOR_PROPS]) scene.add(buildProp(prop))
    this.buildLandmarks()

    const onVillages = (villages: Village[]) => {
      this.villages = villages
      this.districts = assignDistricts(villages.map((village) => village.id))
      this.redrawVillages()
      this.relayoutAvatars()
    }
    const onHistory = ({ villageId, points }: VillageHistory) => {
      this.histories.set(villageId, points)
      this.refreshSigns()
    }
    const onAvatars = (avatars: AvatarSummary[]) => this.syncAvatars(avatars)
    const onMoved = ({ avatarId, to }: AvatarMoved) => {
      const avatar = this.avatars.get(avatarId)
      if (!avatar) return // React reloads `/avatars` for unknown ids.
      avatar.villageId = to
      this.relayoutAvatars()
    }
    const onArenaMessage = (message: ArenaMessage) => this.avatars.get(message.avatar_id)?.label.say(message.text)
    const onMilestone = ({ avatarId, level, text }: AvatarMilestone) => {
      const avatar = this.avatars.get(avatarId)
      if (!avatar) return // not in the town yet; the next `/avatars` load has its level
      avatar.label.setText(nameTag(avatar.displayName, level))
      avatar.label.celebrate(text)
    }
    const onSpawnClone = (clone: AvatarSummary) => {
      if (this.avatars.has(clone.id)) return
      this.avatars.set(clone.id, this.createAvatar(clone))
      this.relayoutAvatars()
    }

    townBus.on('villages:updated', onVillages)
    townBus.on('village:history', onHistory)
    townBus.on('avatars:updated', onAvatars)
    townBus.on('avatar:moved', onMoved)
    townBus.on('arena:message', onArenaMessage)
    townBus.on('avatar:milestone', onMilestone)
    townBus.on('demo:spawn_clone', onSpawnClone)
    // Without this a second town (StrictMode, hot reload) would leave the
    // first one listening and drawing into a dead scene.
    this.unsubscribe = () => {
      townBus.off('villages:updated', onVillages)
      townBus.off('village:history', onHistory)
      townBus.off('avatars:updated', onAvatars)
      townBus.off('avatar:moved', onMoved)
      townBus.off('arena:message', onArenaMessage)
      townBus.off('avatar:milestone', onMilestone)
      townBus.off('demo:spawn_clone', onSpawnClone)
    }
    this.staleTimer = window.setInterval(() => this.redrawVillages(), STALE_REFRESH_MS)
  }

  /** Advances walks and animations. Call once per frame. */
  update(deltaSeconds: number): void {
    for (const avatar of this.avatars.values()) {
      if (avatar.walk) this.stepWalk(avatar, avatar.walk, deltaSeconds)
      avatar.body.update(deltaSeconds)
    }
  }

  /**
   * The ground point a `camera:focus` asks for, or `null` when its target is
   * not in the town (unknown id) or it is a plain reset.
   */
  focusPoint(focus: CameraFocus): GroundPoint | null {
    if (focus.avatarId) {
      const avatar = this.avatars.get(focus.avatarId)
      return avatar ? toGround(avatar.position) : null
    }
    if (focus.villageId) {
      const district = this.districts.get(focus.villageId)
      return district ? rectCentre(district.building) : null
    }
    if (focus.landmarkId) {
      const landmark = LANDMARKS.find((candidate) => candidate.id === focus.landmarkId)
      return landmark ? rectCentre(landmark.prop.tiles) : null
    }
    if (focus.x !== undefined && focus.y !== undefined) return toGround({ x: focus.x, y: focus.y })
    return null
  }

  /** Stops listening and timers, and removes the characters. The caller frees the scene itself. */
  dispose(): void {
    this.unsubscribe()
    window.clearInterval(this.staleTimer)
    for (const avatar of this.avatars.values()) this.removeAvatar(avatar)
    this.avatars.clear()
  }

  // ---- landmarks ----

  /** Makes `object` clickable and hangs a clickable name above it. */
  private addLandmark(object: Object3D, tiles: TileRect, label: string, onClick: () => void): void {
    object.userData.onClick = onClick
    this.scene.add(object)
    this.pickables.push(object)

    const sign = createLandmarkSign(label, onClick)
    const centre = rectCentre(tiles)
    sign.position.set(centre.x, topOf(object) + LABEL_GAP, object.position.z)
    this.scene.add(sign)
  }

  /** The court fountain (opens the Arena) and the landmark buildings (open their panel). */
  private buildLandmarks(): void {
    this.addLandmark(buildProp(COURT_STATUE), COURT_STATUE.tiles, '💬 Arena', () => townBus.emit('arena:selected'))
    for (const landmark of LANDMARKS) {
      this.addLandmark(buildProp(landmark.prop), landmark.prop.tiles, landmark.label, () =>
        townBus.emit('landmark:selected', landmark.id),
      )
    }
  }

  // ---- villages ----

  /** Rebuilds the district buildings (their look depends on `kind` and `stale`) and their signs. */
  private redrawVillages(): void {
    for (const building of this.villageBuildings) {
      building.removeFromParent()
      this.pickables.splice(this.pickables.indexOf(building), 1)
    }
    this.villageBuildings = []
    for (const sign of this.villageSigns.values()) sign.object.removeFromParent()
    this.villageSigns.clear()

    for (const village of this.villages) {
      const district = this.districts.get(village.id)
      if (!district) continue
      const select = () => townBus.emit('village:selected', village.id)

      const building = buildFrame(districtFrame(village.kind), district.building, village.stale)
      building.userData.onClick = select
      this.scene.add(building)
      this.pickables.push(building)
      this.villageBuildings.push(building)

      const sign = new VillageSign(select)
      sign.object.position.set(building.position.x, topOf(building) + LABEL_GAP, building.position.z)
      this.scene.add(sign.object)
      this.villageSigns.set(village.id, sign)
    }
    this.refreshSigns()
  }

  private refreshSigns(): void {
    const now = new Date()
    for (const village of this.villages) {
      this.villageSigns.get(village.id)?.update(village, this.histories.get(village.id) ?? [], now)
    }
  }

  // ---- characters ----

  /**
   * Matches the characters to the latest `/avatars` list: new ones appear at
   * their spot, gone ones are removed, and a changed `main_village` (e.g.
   * after a reconnect) makes the character walk there.
   */
  private syncAvatars(avatars: AvatarSummary[]): void {
    const seen = new Set<string>()
    for (const summary of avatars) {
      seen.add(summary.id)
      const existing = this.avatars.get(summary.id)
      if (existing) {
        existing.villageId = summary.main_village
        // A reload may bring a new level (or a new name).
        existing.displayName = summary.name
        existing.label.setText(nameTag(summary.name, summary.level))
        continue
      }
      this.avatars.set(summary.id, this.createAvatar(summary))
    }
    for (const [id, avatar] of this.avatars) {
      if (seen.has(id)) continue
      this.removeAvatar(avatar)
      this.avatars.delete(id)
    }
    this.relayoutAvatars()
  }

  private createAvatar(summary: AvatarSummary): Avatar3d {
    const select = () => townBus.emit('avatar:selected', summary.id)
    const body = new CharacterBody(knownSprite(summary.sprite))

    const label = new AvatarLabel(nameTag(summary.name, summary.level), select)
    label.object.position.y = CHARACTER_HEIGHT + LABEL_GAP
    body.root.add(label.object)

    const pick = new Mesh(PICK_SHAPE, PICK_MATERIAL)
    pick.position.y = (CHARACTER_HEIGHT * 1.1) / 2
    pick.userData.onClick = select
    body.root.add(pick)
    this.pickables.push(pick)

    this.scene.add(body.root)
    // New characters start in the court; the first layout puts them on their
    // spot without walking (see `relayoutAvatars`).
    const start = { ...COURT.slotsOrigin }
    const avatar: Avatar3d = {
      body,
      label,
      displayName: summary.name,
      villageId: summary.main_village,
      target: start,
      position: start,
    }
    this.moveTo(avatar, start)
    return avatar
  }

  private removeAvatar(avatar: Avatar3d): void {
    for (const child of avatar.body.root.children) {
      const index = this.pickables.indexOf(child)
      if (index !== -1) this.pickables.splice(index, 1)
    }
    avatar.label.dispose()
    avatar.body.dispose()
  }

  private moveTo(avatar: Avatar3d, position: Point): void {
    avatar.position = position
    const ground = toGround(position)
    avatar.body.root.position.set(ground.x, 0, ground.z)
  }

  /**
   * Gives every character its spot (`layoutStandings`) and walks the ones
   * whose spot changed. A character that was never placed jumps there instead.
   */
  private relayoutAvatars(): void {
    const standings = [...this.avatars].map(([id, avatar]) => ({ id, villageId: avatar.villageId }))
    const spots = layoutStandings(standings, this.districts)
    const tiers = layoutTagTiers(standings, this.districts)

    for (const [id, avatar] of this.avatars) {
      // Before the `continue` below: a neighbour leaving can change the
      // tier of a character whose spot did not move.
      avatar.label.setTier(tiers.get(id) ?? 0)

      const spot = spots.get(id)
      if (!spot || (spot.x === avatar.target.x && spot.y === avatar.target.y)) continue

      const previousPlace = avatar.place
      const place = placeOf(avatar.villageId, this.districts)
      avatar.place = place
      avatar.target = spot
      if (!previousPlace) {
        this.moveTo(avatar, spot)
        continue
      }
      this.startWalk(avatar, walkPath(avatar.position, spot, previousPlace, place))
    }
  }

  private startWalk(avatar: Avatar3d, path: Point[]): void {
    if (path.length === 0) return
    const from = { ...avatar.position }
    avatar.walk = { from, path, speed: walkSpeed(pathLength(from, path)), elapsed: 0 }
    avatar.body.setWalking(true)
  }

  private stepWalk(avatar: Avatar3d, walk: Walk, deltaSeconds: number): void {
    walk.elapsed += deltaSeconds
    const moment = walkAt(walk.from, walk.path, walk.speed, walk.elapsed)
    this.moveTo(avatar, moment.position)
    if (moment.heading) avatar.body.root.rotation.y = facingAngle(moment.heading.x, moment.heading.y)
    if (moment.done) {
      avatar.walk = undefined
      avatar.body.setWalking(false)
      // Standing characters face the camera, like the 2D "down" frame.
      avatar.body.root.rotation.y = 0
    }
  }
}
