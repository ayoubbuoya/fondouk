import { useEffect, useState } from 'react'

import { faceUrl } from '../game/sprites'
import { getHallOfFame, type HallOfFame as HallOfFameData } from '../lib/api'
import { formatTimeAgo } from '../lib/format'

interface HallOfFameProps {
  onSelectAvatar: (avatarId: string) => void
}

/**
 * The "Fame" tab of the leaderboard panel (D74): the most experienced
 * characters and the latest levels and badges of the town.
 *
 * Experience is a game score from activity, not money, so `sim` and `real`
 * characters share the list; each row keeps its mode badge. Returns stay on
 * the other tabs.
 */
export function HallOfFame({ onSelectAvatar }: HallOfFameProps) {
  const [data, setData] = useState<HallOfFameData | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    getHallOfFame()
      .then((fame) => {
        if (!cancelled) setData(fame)
      })
      .catch((err) => {
        console.error('loading the hall of fame failed', err)
        if (!cancelled) setError('Failed to load the Hall of Fame.')
      })
    return () => {
      cancelled = true
    }
  }, [])

  if (error) return <p className="panel-error">{error}</p>
  if (!data) return <p className="panel-status">Loading the Hall of Fame…</p>
  if (data.top.length === 0) {
    return <p className="leaderboard-empty">No characters yet. Create one to start earning XP.</p>
  }

  return (
    <div className="fame">
      <section>
        <h3>Most experienced</h3>
        <ol className="fame-list">
          {data.top.map((entry, index) => (
            <li key={entry.id}>
              <button type="button" className="fame-row" onClick={() => onSelectAvatar(entry.id)}>
                <span className="fame-rank">{index + 1}</span>
                <img src={faceUrl(entry.sprite)} alt="" className="leaderboard-face" />
                <span className="fame-name">{entry.name}</span>
                <span className={`badge-${entry.mode}`}>{entry.mode}</span>
                <span className="level-chip">Lv {entry.level}</span>
                <span className="fame-xp">{entry.xp} XP</span>
                <span className="fame-badges" title="Badges unlocked">
                  🏅 {entry.badge_count}
                </span>
              </button>
            </li>
          ))}
        </ol>
      </section>

      <section>
        <h3>Latest unlocks</h3>
        {data.recent.length === 0 ? (
          <p className="panel-muted">Nothing unlocked yet: the first badge comes with the first move.</p>
        ) : (
          <ol className="fame-list">
            {data.recent.map((milestone) => (
              <li key={`${milestone.avatar_id}-${milestone.kind}-${milestone.label}`}>
                <button
                  type="button"
                  className="fame-row"
                  onClick={() => onSelectAvatar(milestone.avatar_id)}
                >
                  <img src={faceUrl(milestone.sprite)} alt="" className="leaderboard-face" />
                  <span className="fame-name">{milestone.avatar_name}</span>
                  <span className="fame-what">
                    {milestone.kind === 'level' ? '⭐ reached' : '🏅 unlocked'} {milestone.label}
                  </span>
                  <time dateTime={milestone.reached_at}>{formatTimeAgo(milestone.reached_at)}</time>
                </button>
              </li>
            ))}
          </ol>
        )}
      </section>
    </div>
  )
}
