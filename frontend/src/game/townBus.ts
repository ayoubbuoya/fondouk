/**
 * The small event bus between React and Phaser (TECH §8, HANDOFF D48).
 *
 * React owns the data (REST + WebSocket) and emits it here; the Phaser scene
 * listens and draws. The scene only reports clicks back. Phaser's own
 * EventEmitter is enough: no new dependency.
 *
 * | Event              | Payload            | From -> To         |
 * | ------------------ | ------------------ | ------------------ |
 * | `town:ready`       | none               | TownScene -> React |
 * | `villages:updated` | `Village[]`        | React -> TownScene |
 * | `village:history`  | `VillageHistory`   | React -> TownScene |
 * | `avatars:updated`  | `AvatarSummary[]`  | React -> TownScene |
 * | `avatar:moved`     | `AvatarMoved`      | React -> TownScene |
 * | `avatar:selected`  | avatar id          | TownScene -> React |
 * | `village:selected` | village id         | TownScene -> React |
 * | `arena:selected`   | none               | TownScene -> React |
 * | `landmark:selected`| `LandmarkId`       | TownScene -> React (a landmark building was clicked) |
 * | `arena:message`    | `ArenaMessage`     | React -> TownScene |
 * | `avatar:milestone` | `AvatarMilestone`  | React -> TownScene (a new level or badge, D74) |
 * | `avatars:reload`   | none               | App -> TownCanvas (after a create, D63) |
 */

import { Events } from 'phaser'

import type { ArenaMessage, AvatarSummary, YieldPoint } from '../lib/api'

export type { ArenaMessage, AvatarSummary }

export interface VillageHistory {
  villageId: string
  points: YieldPoint[]
}

/** Focuses the game camera on a target (e.g. during Judge Tour). */
export interface CameraFocus {
  x?: number
  y?: number
  avatarId?: string
  villageId?: string
  landmarkId?: string
  zoom?: number
  reset?: boolean
}

/** From the `/ws` `avatar_move` event: the sprite walks to `to` (`null` = arena). */
export interface AvatarMoved {
  avatarId: string
  to: string | null
}

/** From the `/ws` `milestone` event: the name tag shows `level`, `text` floats up. */
export interface AvatarMilestone {
  avatarId: string
  level: number
  /** Already worded (`lib/progression.ts` `celebrationText`). */
  text: string
}

export const townBus = new Events.EventEmitter()
