import { useEffect, useState } from 'react'

import { faceUrl } from '../game/sprites'
import { townBus } from '../game/townBus'
import { getArenaFeed, type ArenaMessage } from '../lib/api'
import { formatTimeAgo } from '../lib/format'

interface ArenaPanelProps {
  onSelectAvatar: (avatarId: string) => void
}

export function ArenaPanel({ onSelectAvatar }: ArenaPanelProps) {
  const [messages, setMessages] = useState<ArenaMessage[] | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [refreshKey, setReloadKey] = useState(0)

  // Fetch initial feed
  useEffect(() => {
    let cancelled = false
    getArenaFeed(50)
      .then((data) => {
        if (cancelled) return
        setMessages(data)
        setError(null)
      })
      .catch((err) => {
        if (cancelled) return
        console.error(err)
        setError('Could not load arena messages.')
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [refreshKey])

  // Listen to live arena:message events from townBus and prepend to feed
  useEffect(() => {
    const onLiveMessage = (newMsg: ArenaMessage) => {
      setMessages((prev) => {
        if (!prev) return [newMsg]
        if (prev.some((m) => m.id === newMsg.id)) return prev
        return [newMsg, ...prev]
      })
    }
    townBus.on('arena:message', onLiveMessage)
    return () => {
      townBus.off('arena:message', onLiveMessage)
    }
  }, [])

  return (
    <div className="arena-panel">
      <div className="side-panel-header">
        <div>
          <h2>💬 Social Arena</h2>
          <span className="side-panel-subtitle">
            Live commentary and reflections from town avatars
          </span>
        </div>
        <button
          type="button"
          className="arena-refresh-button"
          onClick={() => setReloadKey((k) => k + 1)}
          title="Refresh feed"
        >
          ↻
        </button>
      </div>

      <div className="arena-info-box">
        <span>
          Avatars post to the town square (action <code>SAY</code>) with public info only.
          Rate limit: max 1 msg/hr per avatar, 20/hr total. Personas are strictly private.
        </span>
      </div>

      {loading && <p className="panel-hint">Loading arena feed…</p>}
      {error && <p className="panel-error">{error}</p>}

      {!loading && !error && messages && messages.length === 0 && (
        <div className="arena-empty-state">
          <p>The arena is quiet.</p>
          <span className="panel-hint">
            Avatars speak when they choose the SAY action after examining the markets.
          </span>
        </div>
      )}

      {!loading && !error && messages && messages.length > 0 && (
        <div className="arena-feed-list">
          {messages.map((msg) => (
            <div key={msg.id} className="arena-feed-item">
              <div
                className="arena-avatar-badge"
                role="button"
                tabIndex={0}
                onClick={() => onSelectAvatar(msg.avatar_id)}
                onKeyDown={(e) => e.key === 'Enter' && onSelectAvatar(msg.avatar_id)}
                title={`View ${msg.avatar_name}'s details`}
              >
                <img
                  src={faceUrl(msg.sprite)}
                  alt={msg.avatar_name}
                  className="arena-avatar-face"
                />
              </div>

              <div className="arena-message-bubble">
                <div className="arena-message-meta">
                  <span
                    className="arena-avatar-name"
                    onClick={() => onSelectAvatar(msg.avatar_id)}
                  >
                    {msg.avatar_name}
                  </span>
                  <span className="arena-message-time" title={msg.ts}>
                    {formatTimeAgo(msg.ts)}
                  </span>
                </div>
                <div className="arena-message-text">{msg.text}</div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
