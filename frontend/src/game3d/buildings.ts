/**
 * The medina's 3D art, made in code from simple shapes (D76): no CC0 medina
 * model pack exists, and a medina is mostly plaster boxes, flat roofs, domes
 * and arches anyway.
 *
 * `medinaLayout.ts` still owns *where* things are. Each of its `FrameName`s
 * (a piece of the 2D `house.png`) has a builder here that makes the 3D
 * version of that piece.
 *
 * ```text
 *   a 2D frame is a FRONT view            its 3D footprint on the ground
 *   ┌────────┐  h tiles tall              ┌────────┐ ─┐
 *   │  roof  │                            │        │  │ depth (chosen by the builder)
 *   │ ▢ ▟▙ ▢ │                            └──door──┘ ─┘
 *   └────────┘ <- bottom edge = the door   ^ front edge = the bottom edge of the 2D rectangle
 *     w tiles
 * ```
 *
 * So a building keeps its 2D width, stands with its front on the bottom edge
 * of its layout rectangle, and goes back (north) by its own depth. Characters
 * stand in front of doors exactly as in 2D.
 */

import {
  BoxGeometry,
  CanvasTexture,
  ConeGeometry,
  CylinderGeometry,
  Group,
  Mesh,
  MeshStandardMaterial,
  PlaneGeometry,
  SphereGeometry,
  SRGBColorSpace,
  type BufferGeometry,
} from 'three'

import { FRAMES, WORLD_TILES, type FrameName, type Prop, type TileRect } from '../game/medinaLayout'
import { pavedMask } from '../game/townMap'

// ---- colours ----

const SAND = 0xe6b673
const PLASTER = 0xf0d9b0
const PLASTER_WARM = 0xe8c394
const PLASTER_WHITE = 0xf7efe0
const BRICK = 0xb5694a
const STONE = 0x9c8f86
const WALL = 0xd2a878
const WOOD = 0x7a4f2c
const WOOD_DARK = 0x4a2f1c
const TERRACOTTA = 0xc8512f
const ORANGE = 0xe08a2e
const TEAL = 0x2e8c84
const GOLD = 0xe8b45a
const WATER = 0x4fb3d9
const LEAF = 0x4f9a4a
const CLOTH_LIGHT = 0xf3ead8

// ---- shared shapes and materials ----
// Every mesh reuses these unit shapes and is scaled to size, so the whole
// town needs only a handful of geometries on the graphics card.

const BOX = new BoxGeometry(1, 1, 1)
const CYLINDER = new CylinderGeometry(0.5, 0.5, 1, 14)
const PYRAMID = new ConeGeometry(0.5, 1, 4)
const SPHERE = new SphereGeometry(0.5, 14, 10)
/** The top half of a sphere, standing on its flat side. */
const DOME = new SphereGeometry(0.5, 16, 8, 0, Math.PI * 2, 0, Math.PI / 2)

const materials = new Map<number, MeshStandardMaterial>()

/** One material per colour, shared by every mesh of that colour. */
function material(color: number): MeshStandardMaterial {
  let found = materials.get(color)
  if (!found) {
    found = new MeshStandardMaterial({ color, roughness: 0.9, metalness: 0, flatShading: true })
    materials.set(color, found)
  }
  return found
}

/** A washed-out version of a colour, for a village whose data is stale. */
function greyed(color: number): number {
  const r = (color >> 16) & 0xff
  const g = (color >> 8) & 0xff
  const b = color & 0xff
  const grey = 0.3 * r + 0.59 * g + 0.11 * b
  const mix = (channel: number) => Math.round(channel * 0.25 + grey * 0.75 * 0.8)
  return (mix(r) << 16) | (mix(g) << 8) | mix(b)
}

/**
 * Adds one scaled shape to `group`. `y` is the height of the shape's
 * **bottom**, so "a box standing on the ground" is simply `y = 0`.
 */
function add(
  group: Group,
  geometry: BufferGeometry,
  color: number,
  size: [number, number, number],
  at: [number, number, number],
): Mesh {
  const mesh = new Mesh(geometry, material(color))
  mesh.scale.set(size[0], size[1], size[2])
  // The dome's flat side is already at its origin; the other shapes are centred.
  const lift = geometry === DOME ? 0 : size[1] / 2
  mesh.position.set(at[0], at[1] + lift, at[2])
  mesh.castShadow = true
  mesh.receiveShadow = true
  group.add(mesh)
  return mesh
}

// ---- buildings ----

/** A built piece: the group (origin = middle of its footprint, on the ground) and how deep it is. */
interface Built {
  object: Group
  depth: number
}

interface HouseStyle {
  wall: number
  /** Colour of the roof edge, the awning and the window frames. */
  trim: number
  door: number
  /** A cloth roof over the door. */
  awning?: boolean
  /** A smaller second floor, set back. */
  upperFloor?: boolean
  dome?: number
  chimney?: boolean
}

const STOREY = 1.8

/**
 * A flat-roofed medina house, `w` wide and `d` deep, front towards +z.
 * `tone` lets a stale village draw the same house washed out.
 */
function house(w: number, d: number, style: HouseStyle, tone: (color: number) => number): Group {
  const group = new Group()
  const front = d / 2
  add(group, BOX, tone(style.wall), [w * 0.94, STOREY, d * 0.94], [0, 0, 0])
  // Roof edge (parapet) and little corner blocks, the look of a kasbah.
  add(group, BOX, tone(style.trim), [w, 0.16, d], [0, STOREY, 0])
  for (const sx of [-1, 1]) {
    for (const sz of [-1, 1]) {
      add(group, BOX, tone(style.wall), [0.3, 0.3, 0.3], [sx * (w / 2 - 0.15), STOREY + 0.16, sz * (d / 2 - 0.15)])
    }
  }
  // Door: a dark slab with a round top, slightly in front of the wall.
  add(group, BOX, tone(style.door), [0.7, 0.9, 0.1], [0, 0, front - 0.02])
  add(group, SPHERE, tone(style.door), [0.7, 0.7, 0.1], [0, 0.55, front - 0.02])
  if (w >= 3) {
    for (const side of [-1, 1]) {
      add(group, BOX, tone(style.trim), [0.5, 0.5, 0.08], [side * (w / 2 - 0.75), 0.95, front - 0.03])
      add(group, BOX, tone(WOOD_DARK), [0.36, 0.36, 0.1], [side * (w / 2 - 0.75), 1.02, front - 0.03])
    }
  }
  if (style.awning) {
    const cloth = add(group, BOX, tone(style.trim), [w * 0.6, 0.06, 0.9], [0, 1.4, front + 0.35])
    cloth.rotation.x = 0.35
  }
  if (style.upperFloor) {
    add(group, BOX, tone(style.wall), [w * 0.55, STOREY * 0.8, d * 0.6], [-w * 0.15, STOREY + 0.16, -d * 0.12])
    add(group, BOX, tone(style.trim), [w * 0.6, 0.14, d * 0.66], [-w * 0.15, STOREY * 1.8 + 0.16, -d * 0.12])
  }
  if (style.dome !== undefined) {
    const size = Math.min(w, d) * 0.6
    const base = style.upperFloor ? STOREY * 1.8 + 0.3 : STOREY + 0.16
    const x = style.upperFloor ? -w * 0.15 : 0
    add(group, DOME, tone(style.dome), [size, size, size], [x, base, style.upperFloor ? -d * 0.12 : 0])
    add(group, SPHERE, tone(GOLD), [0.14, 0.3, 0.14], [x, base + size / 2 - 0.04, style.upperFloor ? -d * 0.12 : 0])
  }
  if (style.chimney) {
    add(group, BOX, tone(STONE), [0.5, 1.1, 0.5], [w / 2 - 0.6, STOREY, -d / 2 + 0.6])
    add(group, BOX, tone(WOOD_DARK), [0.36, 0.1, 0.36], [w / 2 - 0.6, STOREY + 1.1, -d / 2 + 0.6])
  }
  return group
}

/** A market stall: 4 posts, a cloth roof, a table with goods. */
function stall(w: number, cloth: number): Group {
  const group = new Group()
  for (const sx of [-1, 1]) {
    for (const sz of [-1, 1]) add(group, BOX, WOOD, [0.1, 1.5, 0.1], [sx * (w / 2 - 0.15), 0, sz * 0.45])
  }
  const roof = add(group, BOX, cloth, [w, 0.08, 1.3], [0, 1.45, 0.05])
  roof.rotation.x = 0.18
  add(group, BOX, WOOD, [w * 0.85, 0.6, 0.7], [0, 0, 0])
  add(group, SPHERE, ORANGE, [0.3, 0.26, 0.3], [-w * 0.22, 0.6, 0.05])
  add(group, SPHERE, TERRACOTTA, [0.3, 0.26, 0.3], [0, 0.6, -0.05])
  add(group, SPHERE, LEAF, [0.3, 0.26, 0.3], [w * 0.22, 0.6, 0.08])
  return group
}

/** A stepped stone base with something on top. */
function pedestal(group: Group, size: number): number {
  add(group, BOX, STONE, [size, 0.3, size], [0, 0, 0])
  add(group, BOX, STONE, [size * 0.7, 0.5, size * 0.7], [0, 0.3, 0])
  return 0.8
}

/** A small figure on a pedestal: a body and a head, in one colour. */
function statue(color: number): Group {
  const group = new Group()
  const top = pedestal(group, 1.3)
  add(group, SPHERE, color, [0.7, 0.9, 0.6], [0, top, 0])
  add(group, SPHERE, color, [0.45, 0.45, 0.45], [0, top + 0.8, 0])
  return group
}

/** The Hall of Fame monument: a tall gold needle with an orb. */
function monument(): Group {
  const group = new Group()
  add(group, BOX, STONE, [2, 0.35, 2], [0, 0, 0])
  add(group, BOX, PLASTER_WHITE, [1.4, 0.6, 1.4], [0, 0.35, 0])
  add(group, PYRAMID, GOLD, [0.9, 3.2, 0.9], [0, 0.95, 0])
  add(group, SPHERE, GOLD, [0.55, 0.55, 0.55], [0, 4.0, 0])
  return group
}

/** The court fountain (the "Arena" landmark): a basin, water, a column and a gold orb. */
function fountain(): Group {
  const group = new Group()
  add(group, CYLINDER, STONE, [2, 0.45, 2], [0, 0, 0])
  add(group, CYLINDER, WATER, [1.7, 0.06, 1.7], [0, 0.42, 0])
  add(group, CYLINDER, PLASTER_WHITE, [0.35, 1.3, 0.35], [0, 0.4, 0])
  add(group, CYLINDER, STONE, [0.9, 0.12, 0.9], [0, 1.7, 0])
  add(group, SPHERE, GOLD, [0.5, 0.5, 0.5], [0, 1.8, 0])
  return group
}

function pillar(): Group {
  const group = new Group()
  add(group, BOX, STONE, [0.6, 0.2, 0.6], [0, 0, 0])
  add(group, CYLINDER, PLASTER_WHITE, [0.4, 2, 0.4], [0, 0.2, 0])
  add(group, BOX, STONE, [0.6, 0.2, 0.6], [0, 2.2, 0])
  return group
}

/** A clay pot, optionally with something in it. */
function pot(filling?: number): Group {
  const group = new Group()
  add(group, SPHERE, TERRACOTTA, [0.6, 0.55, 0.6], [0, 0, 0])
  add(group, CYLINDER, TERRACOTTA, [0.42, 0.14, 0.42], [0, 0.48, 0])
  if (filling !== undefined) add(group, SPHERE, filling, [0.36, 0.2, 0.36], [0, 0.52, 0])
  return group
}

function barrel(open: boolean): Group {
  const group = new Group()
  add(group, CYLINDER, WOOD, [0.6, 0.75, 0.6], [0, 0, 0])
  add(group, CYLINDER, WOOD_DARK, [0.64, 0.08, 0.64], [0, 0.15, 0])
  add(group, CYLINDER, WOOD_DARK, [0.64, 0.08, 0.64], [0, 0.52, 0])
  if (open) add(group, CYLINDER, WATER, [0.5, 0.04, 0.5], [0, 0.73, 0])
  return group
}

function crate(goods: number): Group {
  const group = new Group()
  add(group, BOX, WOOD, [0.7, 0.45, 0.7], [0, 0, 0])
  for (const [x, z] of [
    [-0.15, -0.12],
    [0.14, -0.1],
    [0, 0.14],
  ]) {
    add(group, SPHERE, goods, [0.26, 0.24, 0.26], [x, 0.4, z])
  }
  return group
}

function sack(): Group {
  const group = new Group()
  add(group, SPHERE, CLOTH_LIGHT, [0.6, 0.7, 0.5], [0, 0, 0])
  add(group, SPHERE, WOOD, [0.18, 0.16, 0.18], [0, 0.62, 0])
  return group
}

function jar(): Group {
  const group = new Group()
  add(group, SPHERE, PLASTER_WARM, [0.45, 0.7, 0.45], [0, 0, 0])
  add(group, CYLINDER, PLASTER_WARM, [0.22, 0.2, 0.22], [0, 0.6, 0])
  return group
}

function banner(cloth: number): Group {
  const group = new Group()
  add(group, CYLINDER, WOOD_DARK, [0.08, 2.4, 0.08], [0, 0, 0])
  add(group, BOX, cloth, [0.5, 1.1, 0.05], [0.3, 1.2, 0])
  add(group, SPHERE, GOLD, [0.16, 0.16, 0.16], [0, 2.38, 0])
  return group
}

function tent(): Group {
  const group = new Group()
  const cloth = add(group, PYRAMID, CLOTH_LIGHT, [2.4, 1.5, 2.4], [0, 0, 0])
  // A 4-sided cone has its corners on the axes: turn it so the sides face them.
  cloth.rotation.y = Math.PI / 4
  add(group, BOX, WOOD_DARK, [0.5, 0.8, 0.06], [0, 0, 0.86])
  return group
}

/** A palm: the desert's version of the 2D bush. */
function palm(): Group {
  const group = new Group()
  add(group, CYLINDER, WOOD, [0.22, 2.2, 0.22], [0, 0, 0])
  for (let i = 0; i < 6; i++) {
    const angle = (i / 6) * Math.PI * 2
    const leaf = add(group, BOX, LEAF, [0.3, 0.06, 1.3], [Math.sin(angle) * 0.55, 2.05, Math.cos(angle) * 0.55])
    leaf.rotation.y = angle
    leaf.rotation.x = 0.45
  }
  add(group, SPHERE, LEAF, [0.4, 0.3, 0.4], [0, 2.1, 0])
  return group
}

const WALL_HEIGHT = 1.9

/** One tile of the town wall, running along x (`alongX`) or along z. */
function wallPiece(alongX: boolean): Group {
  const group = new Group()
  const size: [number, number, number] = alongX ? [1, WALL_HEIGHT, 0.6] : [0.6, WALL_HEIGHT, 1]
  add(group, BOX, WALL, size, [0, 0, 0])
  // One tooth per tile: side by side they make the notched top of a rampart.
  add(group, BOX, WALL, alongX ? [0.5, 0.3, 0.6] : [0.6, 0.3, 0.5], [0, WALL_HEIGHT, 0])
  return group
}

function tower(): Group {
  const group = new Group()
  add(group, BOX, WALL, [2, 3.1, 2], [0, 0, 0])
  add(group, BOX, PLASTER_WARM, [2.2, 0.2, 2.2], [0, 3.1, 0])
  for (const sx of [-1, 1]) {
    for (const sz of [-1, 1]) add(group, BOX, WALL, [0.5, 0.4, 0.5], [sx * 0.8, 3.3, sz * 0.8])
  }
  return group
}

/** The town gate: two piers, a beam across, and a banner colour on top. */
function gate(): Group {
  const group = new Group()
  for (const side of [-1, 1]) add(group, BOX, WALL, [0.9, 3.2, 1], [side * 1.5, 0, 0])
  add(group, BOX, WALL, [3.9, 0.8, 1], [0, 2.6, 0])
  add(group, BOX, TERRACOTTA, [4.1, 0.16, 1.1], [0, 3.4, 0])
  for (const x of [-1.6, -0.8, 0, 0.8, 1.6]) add(group, BOX, WALL, [0.4, 0.35, 1], [x, 3.56, 0])
  return group
}

function kiln(): Group {
  const group = new Group()
  add(group, CYLINDER, BRICK, [2.4, 1.6, 2.4], [0, 0, 0])
  add(group, DOME, BRICK, [2.4, 2, 2.4], [0, 1.6, 0])
  add(group, CYLINDER, WOOD_DARK, [0.5, 0.5, 0.5], [0, 2.4, 0])
  add(group, SPHERE, WOOD_DARK, [0.8, 0.9, 0.12], [0, 0, 1.2])
  return group
}

const same = (color: number) => color

/**
 * The 3D version of each 2D frame. `w` is the frame's width in tiles;
 * `tone` recolours it (used for stale villages).
 */
const BUILDERS: Record<FrameName, (w: number, tone: (color: number) => number) => Built> = {
  houseRed: (w, tone) => ({
    object: house(w, 2.6, { wall: PLASTER, trim: TERRACOTTA, door: WOOD_DARK, awning: true, dome: PLASTER_WHITE }, tone),
    depth: 2.6,
  }),
  houseOrange: (w, tone) => ({
    object: house(w, 2.6, { wall: PLASTER_WARM, trim: ORANGE, door: TEAL, awning: true, upperFloor: true }, tone),
    depth: 2.6,
  }),
  houseSand: (w, tone) => ({ object: house(w, 2.4, { wall: PLASTER_WARM, trim: WALL, door: WOOD }, tone), depth: 2.4 }),
  shop: (w, tone) => ({
    object: house(w, 2.2, { wall: PLASTER_WHITE, trim: TEAL, door: WOOD_DARK, awning: true }, tone),
    depth: 2.2,
  }),
  smithy: (w, tone) => ({
    object: house(w, 2.4, { wall: STONE, trim: WOOD_DARK, door: WOOD_DARK, chimney: true }, tone),
    depth: 2.4,
  }),
  dome: (w, tone) => ({
    object: house(w, 2.4, { wall: PLASTER_WHITE, trim: PLASTER_WARM, door: TEAL, dome: TEAL }, tone),
    depth: 2.4,
  }),
  bigHouse: (w, tone) => ({
    object: house(w, 3.2, { wall: PLASTER, trim: TERRACOTTA, door: WOOD_DARK, upperFloor: true, dome: PLASTER_WHITE }, tone),
    depth: 3.2,
  }),
  kiln: () => ({ object: kiln(), depth: 2.6 }),
  hut: (w, tone) => ({
    object: house(w, 3, { wall: PLASTER_WARM, trim: ORANGE, door: WOOD, upperFloor: true }, tone),
    depth: 3,
  }),
  brickHouse: (w, tone) => ({ object: house(w, 2.2, { wall: BRICK, trim: WOOD_DARK, door: WOOD_DARK }, tone), depth: 2.2 }),
  arch: () => ({ object: gate(), depth: 1 }),
  statueBear: () => ({ object: monument(), depth: 2 }),
  statueOrb: () => ({ object: fountain(), depth: 2 }),
  statueBuddha: () => ({ object: statue(GOLD), depth: 1.4 }),
  statueFrog: () => ({ object: statue(LEAF), depth: 1.4 }),
  statueKnight: () => ({ object: statue(STONE), depth: 1.4 }),
  pillar: () => ({ object: pillar(), depth: 0.7 }),
  stallLight: (w) => ({ object: stall(w, CLOTH_LIGHT), depth: 1.3 }),
  stallDark: (w) => ({ object: stall(w, TERRACOTTA), depth: 1.3 }),
  potWater: () => ({ object: pot(WATER), depth: 1 }),
  potGreens: () => ({ object: pot(LEAF), depth: 1 }),
  potRed: () => ({ object: pot(TERRACOTTA), depth: 1 }),
  barrel: () => ({ object: barrel(false), depth: 1 }),
  barrelOpen: () => ({ object: barrel(true), depth: 1 }),
  crateApples: () => ({ object: crate(TERRACOTTA), depth: 1 }),
  crateBread: () => ({ object: crate(GOLD), depth: 1 }),
  sack: () => ({ object: sack(), depth: 1 }),
  jar: () => ({ object: jar(), depth: 1 }),
  bannerRed: () => ({ object: banner(TERRACOTTA), depth: 0.4 }),
  bannerTeal: () => ({ object: banner(TEAL), depth: 0.4 }),
  bannerGreen: () => ({ object: banner(LEAF), depth: 0.4 }),
  bannerPurple: () => ({ object: banner(0x8a5bb5), depth: 0.4 }),
  bannerYellow: () => ({ object: banner(GOLD), depth: 0.4 }),
  campTent: () => ({ object: tent(), depth: 2 }),
  bush: () => ({ object: palm(), depth: 1 }),
  wallH: () => ({ object: wallPiece(true), depth: 1 }),
  wallV: () => ({ object: wallPiece(false), depth: 1 }),
  tower: () => ({ object: tower(), depth: 2 }),
}

/**
 * Builds a frame and puts it on its layout rectangle: centred left-right,
 * front on the rectangle's bottom edge (see the picture at the top).
 *
 * @param stale draws it washed out (a village with stale data)
 */
export function buildFrame(frame: FrameName, tiles: TileRect, stale = false): Group {
  const width = FRAMES[frame][2]
  const { object, depth } = BUILDERS[frame](width, stale ? greyed : same)
  object.position.set(tiles.x + tiles.w / 2, 0, tiles.y + tiles.h - depth / 2)
  return object
}

/** Builds a layout prop where the layout says. */
export function buildProp(prop: Prop): Group {
  return buildFrame(prop.frame, prop.tiles)
}

// ---- ground ----

/** Texture pixels per tile: enough to see paving joints from the closest zoom. */
const GROUND_PX = 16
/** The desert goes on past the world's edge, so the horizon is never a cliff. */
const DESERT_MARGIN = 300

/** A fixed "random" number per tile, so the ground looks the same on every load. */
function tileHash(tx: number, ty: number): number {
  return (tx * 73 + ty * 151 + tx * ty * 7) % 97
}

/** Paints the whole ground as one picture: sand, and paving where `PAVED` covers a tile. */
function paintGround(): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  canvas.width = WORLD_TILES.w * GROUND_PX
  canvas.height = WORLD_TILES.h * GROUND_PX
  const context = canvas.getContext('2d')
  if (!context) throw new Error('2D canvas is not available for the ground texture')

  context.fillStyle = '#e6b673'
  context.fillRect(0, 0, canvas.width, canvas.height)
  const mask = pavedMask()
  for (let ty = 0; ty < WORLD_TILES.h; ty++) {
    for (let tx = 0; tx < WORLD_TILES.w; tx++) {
      const x = tx * GROUND_PX
      const y = ty * GROUND_PX
      const hash = tileHash(tx, ty)
      if (mask[ty][tx]) {
        context.fillStyle = hash % 3 === 0 ? '#f1d9aa' : '#f5e0b6'
        context.fillRect(x, y, GROUND_PX, GROUND_PX)
        // Thin joints between paving stones.
        context.fillStyle = '#d8bd8a'
        context.fillRect(x, y, GROUND_PX, 1)
        context.fillRect(x, y, 1, GROUND_PX)
      } else if (hash < 14) {
        // About 1 sand tile in 7 gets a darker patch, so the desert is not flat.
        context.fillStyle = '#dcaa66'
        context.fillRect(x + (hash % 8), y + ((hash * 3) % 8), 5, 3)
      }
    }
  }
  return canvas
}

/**
 * The ground: one textured plane for the world, on a much larger plain
 * sand plane. Needs a browser (`document`), like the rest of the 3D scene.
 */
export function buildGround(): Group {
  const group = new Group()
  const texture = new CanvasTexture(paintGround())
  texture.colorSpace = SRGBColorSpace
  texture.anisotropy = 4

  const world = new Mesh(
    new PlaneGeometry(WORLD_TILES.w, WORLD_TILES.h),
    new MeshStandardMaterial({ map: texture, roughness: 1 }),
  )
  // A plane stands upright by default: lay it down. The texture's top edge
  // then points north (small z), matching the layout.
  world.rotation.x = -Math.PI / 2
  world.position.set(WORLD_TILES.w / 2, 0, WORLD_TILES.h / 2)
  world.receiveShadow = true
  group.add(world)

  const desert = new Mesh(new PlaneGeometry(DESERT_MARGIN * 2, DESERT_MARGIN * 2), material(SAND))
  desert.rotation.x = -Math.PI / 2
  // Just below the world plane, so the two never flicker where they overlap.
  desert.position.set(WORLD_TILES.w / 2, -0.02, WORLD_TILES.h / 2)
  group.add(desert)
  return group
}
