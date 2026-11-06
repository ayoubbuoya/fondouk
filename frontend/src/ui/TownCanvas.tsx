import { useEffect, useRef, useState } from 'react'

import { createGame } from '../game/createGame'
import { townBus } from '../game/townBus'
import { getVillageHistory, getVillages } from '../lib/api'
import { connectLiveEvents } from '../lib/liveEvents'

/** F2.3: the sparkline covers 7 days. */
const SPARKLINE_DAYS = 7

/**
 * Hosts the Phaser town and feeds it data.
 *
 * ```text
 * mount ─> createGame ─> TownScene.create ─> "town:ready"
 *            ─> GET /villages             ─> "villages:updated"
 *            ─> GET /villages/{id}/history ─> "village:history" (one per village)
 * /ws apr_update ────────────────────────────> "villages:updated"
 * unmount ─> close /ws, destroy the game
 * ```
 */
export function TownCanvas() {
  const containerRef = useRef<HTMLDivElement>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    // React StrictMode (dev) runs this effect twice: the cleanup below
    // destroys the first game, so only one canvas remains.
    const game = createGame(container)
    // Aborted on unmount, so late answers are not drawn into a destroyed game.
    const unmounted = new AbortController()
    const isUnmounted = () => unmounted.signal.aborted

    const loadTown = async () => {
      const villages = await getVillages()
      if (isUnmounted()) return
      setError(null)
      townBus.emit('villages:updated', villages)
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
        console.error('loading villages failed', err)
        if (!isUnmounted()) setError('Cannot reach the Fondouk backend. Is it running?')
      })
    }
    townBus.once('town:ready', onReady)

    const stopLiveEvents = connectLiveEvents((event) => {
      if (event.type === 'apr_update') {
        setError(null)
        townBus.emit('villages:updated', event.villages)
      }
      // `decision` and `avatar_move` are used from Day 7 (sprites).
    })

    return () => {
      unmounted.abort()
      townBus.off('town:ready', onReady)
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
