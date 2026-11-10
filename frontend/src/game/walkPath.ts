/**
 * Where each character stands and how it walks (F8.2, TECH §8).
 * Pure, no Phaser: unit-tested.
 *
 * **Standing.** A character stands at its `main_village` (the village with
 * most of its money), in a small grid on the forecourt below the building's
 * door. All-cash characters (`main_village = null`) stand in the court.
 *
 * **Walking.** Streets are hub-and-spoke (`medinaLayout.ts`): every place has
 * a short street that ends on the main street. A walk to another place is
 * "my street, along the main street, the other street backwards".
 *
 * ```text
 *   [lend USDC]                      [lend USDT]
 *     ● ● ● ●                          ● ● ● ●
 *        │ ①                              ▲ ③        ① down my street
 *   ═════╧════════ main street ═══════════╧════      ② along the main street
 *              ②  ───────────────────►               ③ up the other street
 * ```
 */

import { COURT, type District, type Place, type Point } from './medinaLayout'

export type { Point }

/** What the layout needs to know about one character. */
export interface Standing {
  id: string
  /** `null` = all cash, stands in the court. */
  villageId: string | null
}

const DISTRICT_COLUMNS = 4
const DISTRICT_COLUMN_GAP = 26
const DISTRICT_ROW_GAP = 24
/** First row: just below the door. */
const DISTRICT_FIRST_ROW = 22
/** 3 rows fit on a forecourt; extra characters share spots. */
const DISTRICT_MAX_SLOTS = 12

const COURT_COLUMNS = 4
const COURT_GAP = 30
const COURT_MAX_SLOTS = 16

/** Never slower than this, so short walks do not look rushed. */
const MIN_WALK_SPEED_PX_PER_S = 200
/** The longest walk still ends within F8's "within 5 s". */
const MAX_WALK_SECONDS = 4.5

/** Spot number `index` in a grid of `columns`, centred under `origin`. */
function gridSlot(origin: Point, index: number, columns: number, columnGap: number, rowGap: number): Point {
  const row = Math.floor(index / columns)
  const column = (index % columns) - (columns - 1) / 2 // e.g. -1.5 .. 1.5
  return { x: origin.x + column * columnGap, y: origin.y + row * rowGap }
}

/** Spot number `index` on a district's forecourt. */
export function districtSlot(district: District, index: number): Point {
  const origin = { x: district.slotsOrigin.x, y: district.slotsOrigin.y + DISTRICT_FIRST_ROW }
  return gridSlot(origin, index % DISTRICT_MAX_SLOTS, DISTRICT_COLUMNS, DISTRICT_COLUMN_GAP, DISTRICT_ROW_GAP)
}

/** Spot number `index` in the court (all-cash characters). */
export function courtSlot(index: number): Point {
  return gridSlot(COURT.slotsOrigin, index % COURT_MAX_SLOTS, COURT_COLUMNS, COURT_GAP, COURT_GAP)
}

/**
 * The place a character belongs to: its village's district, or the court
 * when it is all cash or the village has no district (unknown, not loaded).
 */
export function placeOf(villageId: string | null, districts: Map<string, District>): Place {
  return (villageId !== null && districts.get(villageId)) || COURT
}

/**
 * The characters of each place, sorted by id: a character's position in its
 * list is its spot number. Key `null` = the court (all cash, or a village
 * with no district). Sorting makes the same set always give the same result.
 */
function groupByPlace(standings: Standing[], districts: Map<string, District>): Map<string | null, string[]> {
  const groups = new Map<string | null, string[]>()
  for (const standing of standings) {
    const key = standing.villageId !== null && districts.has(standing.villageId) ? standing.villageId : null
    const group = groups.get(key) ?? []
    group.push(standing.id)
    groups.set(key, group)
  }
  for (const ids of groups.values()) ids.sort()
  return groups
}

/**
 * The spot of every character. Characters are sorted by id inside each
 * place, so the same set always gets the same spots.
 */
export function layoutStandings(standings: Standing[], districts: Map<string, District>): Map<string, Point> {
  const spots = new Map<string, Point>()
  for (const [villageId, ids] of groupByPlace(standings, districts)) {
    const district = villageId === null ? undefined : districts.get(villageId)
    ids.forEach((id, index) => {
      spots.set(id, district ? districtSlot(district, index) : courtSlot(index))
    })
  }
  return spots
}

/**
 * How high each character holds its name tag: 0 = just above the head, 1 =
 * one step higher, and so on.
 *
 * Spots in a row are closer together than a name is wide, so tags at one
 * height print over each other. The tier is the character's column in its
 * row, which makes the tags of one row a staircase:
 *
 * ```text
 *             Mehdi          tier 3
 *          Ines              tier 2
 *       Yunus                tier 1
 *    Amira                   tier 0
 *     ☺   ☺   ☺   ☺
 * ```
 *
 * Known limit: a second row (5 or more characters in one place) starts
 * again at tier 0, so its tags rise into the first row.
 */
export function layoutTagTiers(standings: Standing[], districts: Map<string, District>): Map<string, number> {
  const tiers = new Map<string, number>()
  for (const [villageId, ids] of groupByPlace(standings, districts)) {
    // Same spot numbering and wrap-around as `districtSlot` / `courtSlot`.
    const inDistrict = villageId !== null
    const columns = inDistrict ? DISTRICT_COLUMNS : COURT_COLUMNS
    const maxSlots = inDistrict ? DISTRICT_MAX_SLOTS : COURT_MAX_SLOTS
    ids.forEach((id, index) => {
      tiers.set(id, (index % maxSlots) % columns)
    })
  }
  return tiers
}

/**
 * The points to walk through, in order, ending at `to`.
 *
 * - same place: one straight step, because only the spot changed;
 * - another place: down `fromPlace`'s street, along the main street (both
 *   streets end on it), and `toPlace`'s street backwards.
 *
 * Returns `[]` when the character is already there.
 */
export function walkPath(from: Point, to: Point, fromPlace: Place, toPlace: Place): Point[] {
  if (from.x === to.x && from.y === to.y) return []
  if (fromPlace === toPlace) return [to]

  const route = [...fromPlace.street, ...[...toPlace.street].reverse(), to]
  // Drop a point equal to the one before it (zero-length step).
  return route.filter((point, index) => {
    const previous = index === 0 ? from : route[index - 1]
    return point.x !== previous.x || point.y !== previous.y
  })
}

/** Total length of a walk that starts at `from`. */
export function pathLength(from: Point, path: Point[]): number {
  let length = 0
  let previous = from
  for (const point of path) {
    length += Math.hypot(point.x - previous.x, point.y - previous.y)
    previous = point
  }
  return length
}

/**
 * Walking speed for a walk of `length` pixels: the normal speed, or faster
 * when the walk would otherwise take longer than `MAX_WALK_SECONDS`.
 */
export function walkSpeed(length: number): number {
  return Math.max(MIN_WALK_SPEED_PX_PER_S, length / MAX_WALK_SECONDS)
}
