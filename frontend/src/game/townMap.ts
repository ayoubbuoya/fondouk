/**
 * The medina's ground, built as a 2D array of tile numbers from the layout
 * (`medinaLayout.ts`). Pure, no Phaser: unit-tested.
 *
 * ```text
 *   sand everywhere  ->  light paving where a PAVED rectangle covers the tile
 *
 *   ┌───────┐   a paved tile looks at its 4 neighbours and picks one of the
 *   │0  1  2│   9 border tiles: no paving above = top row, no paving on the
 *   │22 23 24│  left = left column, and so on. Paving on all 4 sides = 23.
 *   │44 45 46│
 *   └───────┘
 * ```
 *
 * Tile numbers are frames of `public/assets/tiles/floor.png` (Ninja
 * Adventure `TilesetFloor.png`, 22 tiles per row): the sand set in the
 * top-left corner of the sheet. Picked by eye on 10 Nov.
 */

import { PAVED, TILE_PX, WORLD_TILES } from './medinaLayout'

export const TILES = {
  sand: 110,
  /** Sand with small marks, sprinkled so the desert is not flat. */
  sandDetails: [111, 112, 113, 114],
  /** 3 × 3 border set of light paving on sand: [row][col]. */
  paved: [
    [0, 1, 2],
    [22, 23, 24],
    [44, 45, 46],
  ],
  /** Paving with a crack or a stone, used rarely inside paved areas. */
  pavedDetails: [88, 89],
} as const

export interface TilePoint {
  tx: number
  ty: number
}

/** World pixel -> the tile under it, clamped to the world. */
export function pixelToTile(x: number, y: number): TilePoint {
  const clamp = (value: number, max: number) => Math.min(max - 1, Math.max(0, Math.floor(value / TILE_PX)))
  return { tx: clamp(x, WORLD_TILES.w), ty: clamp(y, WORLD_TILES.h) }
}

/** `mask[ty][tx]` = true where the ground is paved. */
export function pavedMask(): boolean[][] {
  const mask = Array.from({ length: WORLD_TILES.h }, () => new Array<boolean>(WORLD_TILES.w).fill(false))
  for (const area of PAVED) {
    for (let ty = area.y; ty < area.y + area.h; ty++) {
      for (let tx = area.x; tx < area.x + area.w; tx++) {
        if (ty >= 0 && ty < WORLD_TILES.h && tx >= 0 && tx < WORLD_TILES.w) mask[ty][tx] = true
      }
    }
  }
  return mask
}

/**
 * A fixed "random" number per tile: the same tile always looks the same, so
 * the map does not change between page loads.
 */
function tileHash(tx: number, ty: number): number {
  return (tx * 73 + ty * 151 + tx * ty * 7) % 97
}

function sandTile(tx: number, ty: number): number {
  const hash = tileHash(tx, ty)
  // About 1 tile in 8 gets a mark.
  return hash < 12 ? TILES.sandDetails[hash % TILES.sandDetails.length] : TILES.sand
}

/** The border or centre tile for a paved tile, from its 4 neighbours. */
function pavedTile(mask: boolean[][], tx: number, ty: number): number {
  // Outside the world counts as paved, so a street leaving the map has no border there.
  const paved = (x: number, y: number) => mask[y]?.[x] ?? true
  const row = !paved(tx, ty - 1) ? 0 : !paved(tx, ty + 1) ? 2 : 1
  const column = !paved(tx - 1, ty) ? 0 : !paved(tx + 1, ty) ? 2 : 1
  if (row === 1 && column === 1) {
    const hash = tileHash(tx, ty)
    if (hash < 3) return TILES.pavedDetails[hash % TILES.pavedDetails.length]
  }
  return TILES.paved[row][column]
}

/**
 * Builds the ground: sand everywhere, paving for streets, court and forecourts.
 *
 * @returns `rows[ty][tx]` tile numbers, `WORLD_TILES.h` × `WORLD_TILES.w`.
 */
export function buildGroundTiles(): number[][] {
  const mask = pavedMask()
  return mask.map((row, ty) => row.map((paved, tx) => (paved ? pavedTile(mask, tx, ty) : sandTile(tx, ty))))
}
