/**
 * Where each character stands and how it walks (F8.2, TECH §8).
 * Pure, no Phaser: unit-tested.
 *
 * **Standing.** A character stands at its `main_village` (the village with
 * most of its money). Several characters at one village queue on the road
 * in front of it, 3 abreast, towards the arena. All-cash characters
 * (`main_village = null`) stand in a grid inside the arena (the town square).
 *
 * **Walking.** The roads all meet at the arena, so a walk to another place is
 * two straight steps: to the centre, then to the new spot. No pathfinding.
 *
 * ```text
 *   lend USDC ●  slot 0 1 2          ● lend USDT
 *              ╲   3 4 5            ╱
 *                ╲  ①  ┌──────┐   ╱       ① walk to the centre
 *                      │  ✕   │ ②         ② walk to the new slot
 *                      └──────┘
 * ```
 */

import { MAP_PIXELS } from './townMap'

export interface Point {
  x: number
  y: number
}

/** What the layout needs to know about one character. */
export interface Standing {
  id: string
  /** `null` = all cash, stands in the arena. */
  villageId: string | null
}

export const CENTER: Point = { x: MAP_PIXELS / 2, y: MAP_PIXELS / 2 }

/** First queue row: just past the building (96 px tall when drawn ×2). */
const QUEUE_START = 80
const QUEUE_ROW_GAP = 24
const QUEUE_COLUMN_GAP = 22
const QUEUE_COLUMNS = 3
/** Beyond this the queue would reach the arena; extra characters share slots. */
const QUEUE_MAX_SLOTS = 12

const ARENA_COLUMNS = 4
const ARENA_GAP = 30
/** Below the "Arena" label, which sits near the arena's top edge. */
const ARENA_FIRST_ROW_Y = CENTER.y - 20
const ARENA_MAX_SLOTS = 16

/**
 * Spot number `index` in the queue in front of a village at `village`.
 * Rows go towards the centre, columns spread across the road.
 */
export function queueSlot(village: Point, index: number): Point {
  const slot = index % QUEUE_MAX_SLOTS
  const dx = CENTER.x - village.x
  const dy = CENTER.y - village.y
  const length = Math.hypot(dx, dy) || 1
  // Unit vector along the road, and the one across it.
  const along = { x: dx / length, y: dy / length }
  const across = { x: -along.y, y: along.x }

  const row = Math.floor(slot / QUEUE_COLUMNS)
  const column = (slot % QUEUE_COLUMNS) - (QUEUE_COLUMNS - 1) / 2 // -1, 0, 1
  const distance = QUEUE_START + row * QUEUE_ROW_GAP
  return {
    x: village.x + along.x * distance + across.x * column * QUEUE_COLUMN_GAP,
    y: village.y + along.y * distance + across.y * column * QUEUE_COLUMN_GAP,
  }
}

/** Spot number `index` in the arena grid (all-cash characters). */
export function arenaSlot(index: number): Point {
  const slot = index % ARENA_MAX_SLOTS
  const row = Math.floor(slot / ARENA_COLUMNS)
  const column = (slot % ARENA_COLUMNS) - (ARENA_COLUMNS - 1) / 2 // -1.5 .. 1.5
  return { x: CENTER.x + column * ARENA_GAP, y: ARENA_FIRST_ROW_Y + row * ARENA_GAP }
}

/**
 * The spot of every character. Characters are sorted by id inside each
 * group, so the same set always gets the same spots. A `villageId` that is
 * not in `villages` (unknown or not loaded yet) is treated as the arena.
 */
export function layoutStandings(standings: Standing[], villages: Map<string, Point>): Map<string, Point> {
  const groups = new Map<string | null, string[]>()
  for (const standing of standings) {
    const key = standing.villageId !== null && villages.has(standing.villageId) ? standing.villageId : null
    const group = groups.get(key) ?? []
    group.push(standing.id)
    groups.set(key, group)
  }

  const spots = new Map<string, Point>()
  for (const [villageId, ids] of groups) {
    ids.sort()
    ids.forEach((id, index) => {
      const village = villageId === null ? undefined : villages.get(villageId)
      spots.set(id, village ? queueSlot(village, index) : arenaSlot(index))
    })
  }
  return spots
}

/**
 * The points to walk through, in order, ending at `to`.
 *
 * - same place (same village, or cash -> cash): one straight step, because
 *   only the spot in the queue changed;
 * - different place: through the arena centre, which every road reaches.
 *
 * Returns `[]` when the character is already there.
 */
export function walkPath(from: Point, to: Point, samePlace: boolean): Point[] {
  if (from.x === to.x && from.y === to.y) return []
  return samePlace ? [to] : [CENTER, to]
}
