/**
 * The small event bus between React and Phaser (TECH §8, HANDOFF D48).
 *
 * React owns the data (REST + WebSocket) and emits it here; the Phaser scene
 * listens and draws. Phaser's own EventEmitter is enough: no new dependency.
 *
 * | Event              | Payload            | From -> To        |
 * | ------------------ | ------------------ | ----------------- |
 * | `town:ready`       | none               | TownScene -> React |
 * | `villages:updated` | `Village[]`        | React -> TownScene |
 * | `village:history`  | `VillageHistory`   | React -> TownScene |
 */

import { Events } from 'phaser'

import type { YieldPoint } from '../lib/api'

export interface VillageHistory {
  villageId: string
  points: YieldPoint[]
}

export const townBus = new Events.EventEmitter()
