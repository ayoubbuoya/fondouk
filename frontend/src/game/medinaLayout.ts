/**
 * The medina of Fondouk: where everything is. Pure data, no Phaser:
 * unit-tested (`medinaLayout.test.ts`).
 *
 * This file replaces the 2×2 lawn of D53. The map is still written in code
 * (no Tiled editor), but as one layout: districts, streets, landmarks, props.
 *
 * ```text
 *  town coordinates, in tiles (60 × 40); the world adds a desert margin
 *
 *     2 ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 57    ▓ = wall
 *       ▓                    HALL OF FAME                     ▓
 *       ▓   [lend USDC]         ║            [lend USDT]      ▓    [ ] = district
 *       ▓    forecourt          ║             forecourt       ▓        building
 *       ▓        ║        ┌─────╨─────┐           ║           ▓
 *       ▓        ║  SOUK  │   COURT   │  spare    ║   spare   ▓
 *  y19 ─▓════════╩════════│  ✕ HUB    │═══════════╩═══════════▓─  main street
 *       ▓           ║     │ cash grid │     ║                 ▓
 *       ▓           ║     └─────╥─────┘     ║                 ▓
 *       ▓ [stable pool]         ║           [frax-usdc]       ▓
 *       ▓  forecourt ═╝         ║         ╚═ forecourt        ▓
 *       ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ GATE ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
 *                            caravan road
 * ```
 *
 * **Streets are hub-and-spoke.** Every place has a short `street` that ends
 * on the main street (y = `MAIN_STREET_Y`). A walk is "my street, along the
 * main street, your street backwards": no pathfinding needed.
 *
 * Village positions come from here, keyed by village id. The DB columns
 * `villages.map_x / map_y` are no longer read (HANDOFF D68): hand-placed art
 * must match the position to the pixel, so there is one owner.
 */

export const TILE_SIZE = 16
/** Pixel art is drawn at 2×: one tile = 32 world pixels. */
export const TILE_SCALE = 2
export const TILE_PX = TILE_SIZE * TILE_SCALE

/** A point in world pixels. */
export interface Point {
  x: number
  y: number
}

/** A rectangle in world tiles (top-left tile, size in tiles). */
export interface TileRect {
  x: number
  y: number
  w: number
  h: number
}

/** The whole world: the town plus a desert margin around it. */
export const WORLD_TILES = { w: 76, h: 46 } as const
export const WORLD_PIXELS = { w: WORLD_TILES.w * TILE_PX, h: WORLD_TILES.h * TILE_PX } as const

/** Where the town's tile (0, 0) is in the world. */
const TOWN_ORIGIN = { x: 8, y: 3 } as const

/** A rectangle given in town tiles, stored in world tiles. */
function rect(x: number, y: number, w: number, h: number): TileRect {
  return { x: x + TOWN_ORIGIN.x, y: y + TOWN_ORIGIN.y, w, h }
}

/** The world pixel at the top-left corner of a town tile (fractions allowed). */
function at(tx: number, ty: number): Point {
  return { x: (tx + TOWN_ORIGIN.x) * TILE_PX, y: (ty + TOWN_ORIGIN.y) * TILE_PX }
}

/** The town itself (60 × 40 tiles): what the camera shows at first. */
export const TOWN: TileRect = rect(0, 0, 60, 40)
/** The wall ring; everything of the town is inside it. */
export const WALL: TileRect = rect(2, 1, 56, 38)

/** Centre of the court: every walk between two places passes the main street here or nearby. */
export const HUB: Point = at(30, 19)
/** All streets end at this height: the middle of the 2-tile main street. */
export const MAIN_STREET_Y = HUB.y

// ---- ground ----

/**
 * Light-sand paving. Rectangles may overlap; together they form the streets.
 * Every rectangle is at least 2 tiles wide and high, because the tileset has
 * border tiles for edges and corners but not for 1-tile strips.
 */
export const PAVED: TileRect[] = [
  rect(24, 14, 12, 10), // court
  rect(4, 18, 52, 2), // main street, west to east
  rect(29, 5, 2, 38), // north-south street, out through the gate (caravan road)
  rect(26, 5, 8, 3), // Hall of Fame plaza
  rect(9, 10, 8, 4), // lend USDC forecourt
  rect(12, 14, 2, 4), // ... and its street
  rect(43, 10, 8, 4), // lend USDT forecourt
  rect(46, 14, 2, 4),
  rect(9, 29, 10, 4), // stable pool forecourt
  rect(16, 20, 2, 9),
  rect(41, 29, 10, 4), // frax-usdc forecourt
  rect(42, 20, 2, 9),
  rect(16, 16, 8, 2), // souk (marketplace) front
]

// ---- places ----

/** Somewhere characters can stand: a district, or the court. */
export interface Place {
  /** Characters stand in a grid below this point. */
  slotsOrigin: Point
  /** From the standing area to the main street; the last point has y = `MAIN_STREET_Y`. */
  street: Point[]
}

/** A village's home: a building with a forecourt. */
export interface District extends Place {
  /** 4 × 3 tiles: the size of a house in `house.png`. */
  building: TileRect
}

/** A 4 × 3 house whose door (bottom centre) is at town tile (doorX, doorY). */
function district(doorX: number, doorY: number, street: Point[]): District {
  return {
    building: rect(doorX - 2, doorY - 3, 4, 3),
    slotsOrigin: at(doorX, doorY),
    street,
  }
}

/** The 4 launch villages (migration 0002), by village id. */
export const DISTRICTS: Record<string, District> = {
  'rhea-lend-usdc': district(13, 10, [at(13, 13.5), at(13, 19)]),
  'rhea-lend-usdt': district(47, 10, [at(47, 13.5), at(47, 19)]),
  'rhea-dex-stable4': district(12, 29, [at(17, 30.5), at(17, 19)]),
  'rhea-dex-frax-usdc': district(48, 29, [at(43, 30.5), at(43, 19)]),
}

/**
 * Plots for villages this file does not know yet (a 5th village added in
 * the DB). They open straight onto the main street, so an unused plot is
 * just sand: no empty forecourt to explain.
 */
export const SPARE_PLOTS: District[] = [district(40, 18, [at(40, 19)]), district(52, 18, [at(52, 19)])]

/** The court (town square, the "arena"): all-cash characters stand here. */
export const COURT: Place = {
  slotsOrigin: { x: HUB.x, y: HUB.y + 18 },
  street: [HUB],
}

/**
 * Gives every village id its district. Known ids get their own; unknown ids
 * share the spare plots in id order (stable across reloads). An id that gets
 * no plot is left out: its characters stand in the court.
 */
export function assignDistricts(villageIds: string[]): Map<string, District> {
  const assigned = new Map<string, District>()
  const unknown: string[] = []
  for (const id of villageIds) {
    const known = DISTRICTS[id]
    if (known) assigned.set(id, known)
    else unknown.push(id)
  }
  unknown.sort()
  unknown.slice(0, SPARE_PLOTS.length).forEach((id, index) => assigned.set(id, SPARE_PLOTS[index]))
  return assigned
}

// ---- art ----

/**
 * Pieces of `public/assets/tiles/house.png` (Ninja Adventure `TilesetHouse.png`),
 * as [column, row, width, height] in 16 px tiles. Picked by eye on 10 Nov.
 */
export const FRAMES = {
  houseRed: [12, 0, 4, 3],
  houseOrange: [0, 0, 4, 3],
  houseSand: [4, 0, 4, 3],
  shop: [16, 0, 3, 3],
  smithy: [19, 0, 4, 3],
  dome: [23, 0, 3, 3],
  bigHouse: [29, 0, 4, 4],
  kiln: [29, 4, 3, 4],
  hut: [25, 7, 4, 5],
  brickHouse: [19, 19, 3, 3],
  arch: [29, 20, 3, 3],
  statueBear: [1, 15, 2, 3],
  statueOrb: [3, 15, 2, 2],
  statueBuddha: [5, 15, 2, 2],
  statueFrog: [3, 17, 2, 2],
  statueKnight: [5, 17, 2, 2],
  pillar: [0, 15, 1, 3],
  stallLight: [16, 16, 3, 2],
  stallDark: [19, 16, 3, 2],
  potWater: [16, 13, 1, 1],
  potGreens: [17, 13, 1, 1],
  potRed: [18, 13, 1, 1],
  barrel: [18, 15, 1, 1],
  barrelOpen: [17, 15, 1, 1],
  crateApples: [19, 14, 1, 1],
  crateBread: [20, 14, 1, 1],
  sack: [21, 13, 1, 1],
  jar: [17, 20, 1, 1],
  bannerRed: [23, 20, 1, 2],
  bannerTeal: [24, 20, 1, 2],
  bannerGreen: [25, 20, 1, 2],
  bannerPurple: [27, 20, 1, 2],
  bannerYellow: [28, 20, 1, 2],
  campTent: [16, 11, 2, 2],
  bush: [8, 15, 1, 2],
  wallH: [11, 9, 1, 1],
  wallV: [11, 8, 1, 1],
  tower: [8, 8, 2, 2],
} as const satisfies Record<string, readonly [number, number, number, number]>

export type FrameName = keyof typeof FRAMES

/** One piece of art placed in the world. */
export interface Prop {
  frame: FrameName
  /** World tiles it covers (its size is the frame's size). */
  tiles: TileRect
}

/** A prop whose top-left corner is at town tile (x, y); fractions allowed. */
function prop(frame: FrameName, x: number, y: number): Prop {
  const [, , w, h] = FRAMES[frame]
  return { frame, tiles: rect(x, y, w, h) }
}

/** The house drawn for a village, by `villages.kind`. */
export function districtFrame(kind: string): FrameName {
  return kind === 'lending' ? 'houseRed' : 'houseOrange'
}

/** A building that opens a panel when clicked. */
export type LandmarkId = 'marketplace' | 'leaderboard'

export interface Landmark {
  id: LandmarkId
  label: string
  prop: Prop
}

export const LANDMARKS: Landmark[] = [
  { id: 'marketplace', label: '🛒 Souk · Marketplace', prop: prop('shop', 18, 13) },
  { id: 'leaderboard', label: '🏆 Hall of Fame', prop: prop('statueBear', 29, 2) },
]

/** The statue in the court; the "Arena" label sits above it. */
export const COURT_STATUE: Prop = prop('statueOrb', 29, 15)

/** Houses nobody lives in: they make the town feel built-up. Characters never stand on them. */
export const DECOR_BUILDINGS: Prop[] = [
  prop('houseSand', 19, 6),
  prop('dome', 21, 10),
  prop('houseSand', 36, 6),
  prop('brickHouse', 37, 10),
  prop('smithy', 52, 21),
  prop('hut', 21, 26),
  prop('houseSand', 34, 26),
  prop('bigHouse', 35, 31),
  prop('kiln', 4, 21),
  prop('brickHouse', 24, 32),
  prop('houseSand', 4, 13),
  prop('dome', 53, 30),
]

/** Small things: stalls, pots, banners, statues. Purely decorative. */
export const DECOR_PROPS: Prop[] = [
  // Souk: stalls and goods around the marketplace shop.
  prop('stallLight', 21, 14),
  prop('stallDark', 18, 21),
  prop('stallLight', 21.5, 21),
  prop('crateApples', 16.5, 15),
  prop('crateBread', 17, 16),
  prop('sack', 21.5, 16.2),
  prop('potGreens', 22.5, 16.2),
  prop('barrel', 24.2, 21.5),
  prop('jar', 20.8, 23),
  // Court: banners on the corners, pots by the statue.
  prop('bannerRed', 24, 12.2),
  prop('bannerTeal', 27, 12.2),
  prop('bannerGreen', 32, 12.2),
  prop('bannerYellow', 35, 12.2),
  prop('potWater', 28, 16),
  prop('potWater', 31, 16),
  prop('bannerPurple', 24, 23.6),
  prop('bannerRed', 35, 23.6),
  // Hall of Fame: statues flank the bear.
  prop('pillar', 27, 2),
  prop('pillar', 32, 2),
  prop('statueBuddha', 24, 4.5),
  prop('statueKnight', 34, 4.5),
  // Lending districts look like banks: pillars at the door.
  prop('pillar', 10, 7.5),
  prop('pillar', 15, 7.5),
  prop('pillar', 44, 7.5),
  prop('pillar', 49, 7.5),
  // Pool districts look like wells and stores: water, barrels, sacks.
  prop('potWater', 14.2, 27.6),
  prop('barrel', 15.2, 27.8),
  prop('barrelOpen', 9, 28),
  prop('statueFrog', 7, 30),
  prop('potWater', 44.8, 27.6),
  prop('barrel', 43.8, 27.8),
  prop('sack', 50.2, 28),
  prop('statueFrog', 51, 30),
  // Odds and ends along the streets.
  prop('stallDark', 37, 21),
  prop('potRed', 40.2, 21.5),
  prop('crateApples', 36, 21.8),
  prop('barrel', 31.5, 31),
  prop('jar', 27.5, 28),
  prop('sack', 33, 9.5),
  prop('potGreens', 8, 16.5),
  // Outside the walls: a caravan camp on the road.
  prop('campTent', 33, 40.6),
  prop('campTent', 25, 41),
  prop('barrel', 35.2, 41.6),
  prop('bush', 22, 40.4),
  prop('bush', 38, 41),
  prop('bush', -3, 12),
  prop('bush', 62, 25),
  prop('bush', 63, 6),
  prop('bush', -5, 33),
]

/** The gate in the south wall, over the north-south street. */
export const GATE: Prop = prop('arch', 28.5, 36.4)

/**
 * The wall pieces: towers on the 4 corners, wall tiles between them, and a
 * gap in the south wall for the gate.
 */
export function wallProps(): Prop[] {
  const left = WALL.x - TOWN_ORIGIN.x
  const top = WALL.y - TOWN_ORIGIN.y
  const right = left + WALL.w - 1
  const bottom = top + WALL.h - 1
  const gate = { from: 28, to: 31 } // town tiles left open for the arch

  const props: Prop[] = []
  for (let x = left + 2; x <= right - 2; x++) {
    props.push(prop('wallH', x, top + 0.5))
    if (x < gate.from || x > gate.to) props.push(prop('wallH', x, bottom))
  }
  for (let y = top + 2; y <= bottom - 1; y++) {
    props.push(prop('wallV', left + 0.5, y))
    props.push(prop('wallV', right - 0.5, y))
  }
  for (const x of [left, right - 1]) {
    props.push(prop('tower', x, top))
    props.push(prop('tower', x, bottom - 1))
  }
  return props
}

/** Everything characters must not stand on. */
export function solidRects(): TileRect[] {
  return [
    ...Object.values(DISTRICTS).map((d) => d.building),
    ...SPARE_PLOTS.map((d) => d.building),
    ...LANDMARKS.map((landmark) => landmark.prop.tiles),
    ...DECOR_BUILDINGS.map((building) => building.tiles),
    COURT_STATUE.tiles,
  ]
}

/** The pixel rectangle of a tile rectangle. */
export function toPixels(tiles: TileRect): { x: number; y: number; w: number; h: number } {
  return { x: tiles.x * TILE_PX, y: tiles.y * TILE_PX, w: tiles.w * TILE_PX, h: tiles.h * TILE_PX }
}
