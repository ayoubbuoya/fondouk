/**
 * The town's ground, built as a 2D array of tile numbers in code
 * (HANDOFF D53: no Tiled editor). Pure, no Phaser: unit-tested.
 *
 * ```text
 *  25 × 25 tiles, each 16 px drawn ×2 = 32 px  ->  an 800 × 800 canvas
 *
 *   V . . . . . . . . . . V      V = village tile (from the DB map_x/map_y)
 *    ╲                   ╱       ╲ = road: a 2-tile wide 45° staircase
 *      ╲   ┌───────┐   ╱             from each village to the arena
 *        ╲ │ arena │ ╱           arena = 5 × 5 dirt patch with grass edges
 *        ╱ │  5×5  │ ╲
 *      ╱   └───────┘   ╲
 *   V                    V
 * ```
 *
 * Tile numbers are frames of `public/assets/tiles/floor.png` (Ninja
 * Adventure `TilesetFloor.png`, 22 tiles per row), picked by eye on 6 Nov.
 */

export const TILE_SIZE = 16
/** Pixel-art is drawn at 2×: one tile = 32 screen pixels. */
export const TILE_SCALE = 2
export const MAP_TILES = 25
/** Must equal `CANVAS_SIZE` in `TownScene.ts` (800). */
export const MAP_PIXELS = MAP_TILES * TILE_SIZE * TILE_SCALE

export const TILES = {
  grass: 264,
  /** Grass with small details, sprinkled so the field is not flat. */
  grassDetails: [244, 266, 267],
  road: 177,
  /** 3 × 3 autotile of a dirt patch on grass: [row][col]. */
  arena: [
    [154, 155, 156],
    [176, 177, 178],
    [198, 199, 200],
  ],
} as const

/** The arena is 5 × 5 tiles around the centre tile (12, 12). */
export const ARENA_TILES = 5
const CENTER_TILE = Math.floor(MAP_TILES / 2)
const ARENA_MIN = CENTER_TILE - Math.floor(ARENA_TILES / 2)
const ARENA_MAX = ARENA_MIN + ARENA_TILES - 1

export interface TilePoint {
  tx: number
  ty: number
}

/** Screen pixel (0–800) -> the tile under it, clamped to the map. */
export function pixelToTile(x: number, y: number): TilePoint {
  const size = TILE_SIZE * TILE_SCALE
  const clamp = (value: number) => Math.min(MAP_TILES - 1, Math.max(0, Math.floor(value / size)))
  return { tx: clamp(x), ty: clamp(y) }
}

function insideArena(tx: number, ty: number): boolean {
  return tx >= ARENA_MIN && tx <= ARENA_MAX && ty >= ARENA_MIN && ty <= ARENA_MAX
}

/** The arena tile at (tx, ty): corner, edge or centre of the 3 × 3 autotile. */
function arenaTile(tx: number, ty: number): number {
  const part = (value: number) => (value === ARENA_MIN ? 0 : value === ARENA_MAX ? 2 : 1)
  return TILES.arena[part(ty)][part(tx)]
}

/**
 * A fixed "random" pattern: the same tile always gets the same grass, so the
 * map does not flicker when it is rebuilt.
 */
function grassTile(tx: number, ty: number): number {
  const hash = (tx * 7 + ty * 13) % 17
  return hash < TILES.grassDetails.length ? TILES.grassDetails[hash] : TILES.grass
}

/**
 * Builds the ground: grass everywhere, the arena in the middle, and a road
 * from each village tile to the centre. Roads stop at the arena edge so its
 * grass border stays intact.
 *
 * @returns `rows[ty][tx]` tile numbers, `MAP_TILES` × `MAP_TILES`.
 */
export function buildTownTiles(villages: TilePoint[]): number[][] {
  const rows: number[][] = []
  for (let ty = 0; ty < MAP_TILES; ty++) {
    const row: number[] = []
    for (let tx = 0; tx < MAP_TILES; tx++) {
      row.push(insideArena(tx, ty) ? arenaTile(tx, ty) : grassTile(tx, ty))
    }
    rows.push(row)
  }

  for (const village of villages) {
    const dx = CENTER_TILE - village.tx
    const dy = CENTER_TILE - village.ty
    const steps = Math.max(Math.abs(dx), Math.abs(dy))
    for (let step = 0; step <= steps; step++) {
      const tx = village.tx + Math.round((step * dx) / Math.max(steps, 1))
      const ty = village.ty + Math.round((step * dy) / Math.max(steps, 1))
      // Two tiles wide: the tile and its right-hand neighbour.
      for (const x of [tx, tx + 1]) {
        if (x < MAP_TILES && !insideArena(x, ty)) rows[ty][x] = TILES.road
      }
    }
  }
  return rows
}
