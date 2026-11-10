import { useEffect, useRef, useState } from 'react'

import { createGame } from '../game/createGame'
import type { LandmarkId } from '../game/medinaLayout'
import { townBus, type AvatarMilestone, type AvatarMoved } from '../game/townBus'
import { getAvatars, getVillageHistory, getVillages, type AvatarSummary, type Village } from '../lib/api'
import { connectLiveEvents, type LiveEvent } from '../lib/liveEvents'
import { celebrationText } from '../lib/progression'

/** F2.3: the sparkline covers 7 days. */
const SPARKLINE_DAYS = 7

/** What the user clicked on the map. */
export type Selection = { kind: 'avatar'; id: string } | { kind: 'village'; id: string }

interface TownCanvasProps {
  /** A sprite or a building was clicked. */
  onSelect: (selection: Selection) => void
  /** The court statue (the arena) was clicked. */
  onOpenArena?: () => void
  /** A landmark building was clicked: the Souk or the Hall of Fame. */
  onOpenLandmark?: (landmark: LandmarkId) => void
  /** The villages were loaded or a live APY update arrived (for the Hud and the news strip). */
  onVillages?: (villages: Village[]) => void
  /** The character list was (re)loaded. */
  onAvatars?: (avatars: AvatarSummary[]) => void
  /** Every live event, so open panels can refresh (F8.5). */
  onLiveEvent: (event: LiveEvent) => void
}

/**
 * Hosts the Phaser town and feeds it data. Only this component and the scene
 * use `townBus`; the rest of the app gets plain callbacks.
 *
 * ```text
 * mount ─> createGame ─> TownScene.create ─> "town:ready"
 *            ─> GET /villages             ─> "villages:updated"
 *            ─> GET /avatars              ─> "avatars:updated"   (after villages,
 *            ─> GET /villages/{id}/history ─> "village:history"    so sprites never
 * /ws apr_update  ─────────────────────────> "villages:updated"    walk in from the arena)
 * /ws avatar_move ─────────────────────────> "avatar:moved" (unknown id: reload /avatars)
 * /ws decision for an unknown id ──────────> reload /avatars (a new character)
 * /ws milestone ───────────────────────────> "avatar:milestone" (name tag level + floating text)
 * scene "avatar:selected" / "village:selected" ─> onSelect
 * scene "landmark:selected" ─> onOpenLandmark (Souk -> marketplace, Hall of Fame -> leaderboard)
 * unmount ─> close /ws, destroy the game
 * ```
 */
export function TownCanvas({
  onSelect,
  onOpenArena,
  onOpenLandmark,
  onVillages,
  onAvatars,
  onLiveEvent,
}: TownCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const [error, setError] = useState<string | null>(null)
  // Callbacks change on every App render; refs keep the effect below mount-only.
  const onSelectRef = useRef(onSelect)
  const onOpenArenaRef = useRef(onOpenArena)
  const onOpenLandmarkRef = useRef(onOpenLandmark)
  const onVillagesRef = useRef(onVillages)
  const onAvatarsRef = useRef(onAvatars)
  const onLiveEventRef = useRef(onLiveEvent)
  useEffect(() => {
    onSelectRef.current = onSelect
    onOpenArenaRef.current = onOpenArena
    onOpenLandmarkRef.current = onOpenLandmark
    onVillagesRef.current = onVillages
    onAvatarsRef.current = onAvatars
    onLiveEventRef.current = onLiveEvent
  })

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    // React StrictMode (dev) runs this effect twice: the cleanup below
    // destroys the first game, so only one canvas remains.
    const game = createGame(container)
    // Aborted on unmount, so late answers are not drawn into a destroyed game.
    const unmounted = new AbortController()
    const isUnmounted = () => unmounted.signal.aborted
    /** Ids the scene knows, to spot new characters in live events. */
    const knownAvatars = new Set<string>()

    const loadAvatars = async () => {
      const avatars = await getAvatars()
      if (isUnmounted()) return
      knownAvatars.clear()
      for (const avatar of avatars) knownAvatars.add(avatar.id)
      townBus.emit('avatars:updated', avatars)
      onAvatarsRef.current?.(avatars)
    }
    const reloadAvatars = () => {
      loadAvatars().catch((err) => console.warn('reloading avatars failed', err))
    }

    const loadTown = async () => {
      const villages = await getVillages()
      if (isUnmounted()) return
      setError(null)
      townBus.emit('villages:updated', villages)
      onVillagesRef.current?.(villages)
      await loadAvatars()
      // 4 villages = 4 small parallel requests. A failed sparkline only
      // leaves that one village without a line.
      for (const village of villages) {
        getVillageHistory(village.id, SPARKLINE_DAYS)
          .then((points) => {
            if (!isUnmounted()) townBus.emit('village:history', { villageId: village.id, points })
          })
          .catch((err) => console.warn(`history of ${village.id} failed`, err))
      }
    }
    const onReady = () => {
      loadTown().catch((err) => {
        console.error('loading the town failed', err)
        if (!isUnmounted()) setError('Cannot reach the Fondouk backend. Is it running?')
      })
    }
    const onAvatarSelected = (id: string) => onSelectRef.current({ kind: 'avatar', id })
    const onVillageSelected = (id: string) => onSelectRef.current({ kind: 'village', id })
    const onArenaSelected = () => onOpenArenaRef.current?.()
    const onLandmarkSelected = (landmark: LandmarkId) => onOpenLandmarkRef.current?.(landmark)
    townBus.once('town:ready', onReady)
    townBus.on('avatar:selected', onAvatarSelected)
    townBus.on('village:selected', onVillageSelected)
    townBus.on('arena:selected', onArenaSelected)
    townBus.on('landmark:selected', onLandmarkSelected)
    // A newly created character is paused, so no live event announces it.
    townBus.on('avatars:reload', reloadAvatars)

    const stopLiveEvents = connectLiveEvents((event) => {
      switch (event.type) {
        case 'apr_update':
          setError(null)
          townBus.emit('villages:updated', event.villages)
          onVillagesRef.current?.(event.villages)
          break
        case 'avatar_move':
          if (knownAvatars.has(event.avatar_id)) {
            const moved: AvatarMoved = { avatarId: event.avatar_id, to: event.to }
            townBus.emit('avatar:moved', moved)
          } else {
            reloadAvatars()
          }
          break
        case 'decision':
          if (!knownAvatars.has(event.avatar_id)) reloadAvatars()
          break
        case 'arena_message':
          townBus.emit('arena:message', event)
          break
        case 'milestone': {
          // An unknown id needs no reload here: the `decision` event of the
          // same tick arrived just before and already asked for one.
          const milestone: AvatarMilestone = {
            avatarId: event.avatar_id,
            level: event.level,
            text: celebrationText(event.kind, event.label),
          }
          townBus.emit('avatar:milestone', milestone)
          break
        }
      }
      onLiveEventRef.current(event)
    })

    return () => {
      unmounted.abort()
      townBus.off('town:ready', onReady)
      townBus.off('avatar:selected', onAvatarSelected)
      townBus.off('village:selected', onVillageSelected)
      townBus.off('arena:selected', onArenaSelected)
      townBus.off('landmark:selected', onLandmarkSelected)
      townBus.off('avatars:reload', reloadAvatars)
      stopLiveEvents()
      game.destroy(true)
    }
  }, [])

  return (
    <div className="town">
      <div ref={containerRef} className="town-canvas" />
      {error && <p className="town-error">{error}</p>}
    </div>
  )
}
