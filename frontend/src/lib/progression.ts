/**
 * Display helpers for progression (D74): levels, the XP bar and badges.
 * Pure, no DOM: unit-tested.
 *
 * The rules (how much XP, when a badge unlocks) live only in the backend
 * (`backend/src/progression.rs`). The browser shows what the API sends; it
 * never computes XP itself.
 */

/**
 * The name tag above a sprite: the level is a bare number after the name
 * ("Amira 4"). Kept this short on purpose: characters in one district stand
 * close together, and a longer tag ("Amira · Lv 4") made neighbours overlap.
 */
export function nameTag(name: string, level: number): string {
  return `${name} ${level}`
}

/** How full the XP bar is, 0 to 100. A missing or zero span gives an empty bar. */
export function xpPercent(xpIntoLevel: number, xpForNextLevel: number): number {
  if (!(xpForNextLevel > 0)) return 0
  return Math.min(100, Math.max(0, (xpIntoLevel / xpForNextLevel) * 100))
}

/** The short text that floats above a sprite when it reaches a milestone. */
export function celebrationText(kind: 'level' | 'badge', label: string): string {
  return kind === 'level' ? 'LEVEL UP!' : `🏅 ${label}`
}

/**
 * Icon per badge id. **Keep in sync** with `BADGES` in
 * `backend/src/progression.rs`; an id missing here still shows, with the
 * default icon, so a newer backend cannot break the page.
 */
const BADGE_ICONS: Record<string, string> = {
  first_steps: '👣',
  diversified: '🧺',
  storyteller: '📜',
  kept_in_line: '🛡️',
  in_the_green: '🌿',
  veteran: '⏳',
  masters_mark: '🖋️',
  guild_founder: '🏛️',
}

const DEFAULT_BADGE_ICON = '🏅'

export function badgeIcon(id: string): string {
  return BADGE_ICONS[id] ?? DEFAULT_BADGE_ICON
}
