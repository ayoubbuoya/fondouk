/**
 * The small event bus between React and the 3D town (TECH §8, HANDOFF D48, D77).
 *
 * React owns the data (REST + WebSocket) and emits it here; the town
 * (`game3d/Town3d.ts`) listens and draws. The town only reports clicks back.
 * The emitter is a few lines of our own (`EventBus` below): it keeps the
 * `on / off / once / emit` API the bus had before D77.
 *
 * | Event              | Payload            | From -> To         |
 * | ------------------ | ------------------ | ------------------ |
 * | `town:ready`       | none               | town -> React      |
 * | `villages:updated` | `Village[]`        | React -> town      |
 * | `village:history`  | `VillageHistory`   | React -> town      |
 * | `avatars:updated`  | `AvatarSummary[]`  | React -> town      |
 * | `avatar:moved`     | `AvatarMoved`      | React -> town      |
 * | `avatar:selected`  | avatar id          | town -> React      |
 * | `village:selected` | village id         | town -> React      |
 * | `arena:selected`   | none               | town -> React      |
 * | `landmark:selected`| `LandmarkId`       | town -> React      (a landmark building was clicked) |
 * | `arena:message`    | `ArenaMessage`     | React -> town      |
 * | `avatar:milestone` | `AvatarMilestone`  | React -> town      (a new level or badge, D74) |
 * | `avatars:reload`   | none               | App -> TownCanvas (after a create, D63) |
 * | `camera:focus`     | `CameraFocus`      | App / JudgeTour -> town (D75) |
 * | `demo:spawn_clone` | `AvatarSummary`    | JudgeTour -> town (D75) |
 */

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

// Each event carries its own payload type (table above); listeners name it
// in their parameter, so the bus itself stays loosely typed, as before.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Listener = (...args: any[]) => void

/**
 * A minimal event emitter: listeners per event name, called in the order
 * they were added.
 *
 * ```text
 *   on('x', fn)     listeners['x'] = { fn }
 *   once('x', fn)   listeners['x'] = { wrapper }   wrapper calls fn, then removes itself
 *   off('x', fn)    removes fn, or the wrapper made for fn by `once`
 *   emit('x', a)    calls every listener of 'x' with a
 * ```
 */
export class EventBus {
  private readonly listeners = new Map<string, Set<Listener>>()
  /** `once` wrappers by the function the caller passed, so `off(event, fn)` can find them. */
  private readonly onceWrappers = new Map<Listener, Listener>()

  on(event: string, listener: Listener): this {
    let set = this.listeners.get(event)
    if (!set) {
      set = new Set()
      this.listeners.set(event, set)
    }
    set.add(listener)
    return this
  }

  /** Like `on`, but the listener runs at most once. */
  once(event: string, listener: Listener): this {
    const wrapper: Listener = (...args) => {
      this.off(event, listener)
      listener(...args)
    }
    this.onceWrappers.set(listener, wrapper)
    return this.on(event, wrapper)
  }

  /** Removes `listener` from `event`; removing one that was never added does nothing. */
  off(event: string, listener: Listener): this {
    const set = this.listeners.get(event)
    const wrapper = this.onceWrappers.get(listener)
    if (wrapper) {
      set?.delete(wrapper)
      this.onceWrappers.delete(listener)
    }
    set?.delete(listener)
    if (set?.size === 0) this.listeners.delete(event)
    return this
  }

  /**
   * Calls every listener of `event` with `args`.
   *
   * @returns `true` if at least one listener was called
   */
  emit(event: string, ...args: unknown[]): boolean {
    const set = this.listeners.get(event)
    if (!set) return false
    // A copy: a listener may remove itself (or another) while we loop.
    for (const listener of [...set]) listener(...args)
    return true
  }
}

export const townBus = new EventBus()
