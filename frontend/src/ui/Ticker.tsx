import { formatTimeAgo } from '../lib/format'
import type { TickerLine } from '../lib/ticker'

interface TickerProps {
  /** Newest first (`lib/ticker.ts`). */
  lines: TickerLine[]
  onSelectAvatar: (avatarId: string) => void
}

/**
 * The news strip along the bottom of the town: what the characters just did
 * or said. Each line shows its real age and opens its character when clicked.
 */
export function Ticker({ lines, onSelectAvatar }: TickerProps) {
  return (
    <div className="ticker" role="log" aria-label="Town news">
      <span className="ticker-title">Town news</span>
      {lines.length === 0 ? (
        <span className="ticker-empty">Quiet for now: the merchants decide every few minutes.</span>
      ) : (
        <ol className="ticker-lines">
          {lines.map((line) => (
            <li key={line.id}>
              <button
                type="button"
                className={`ticker-line ticker-${line.tone}`}
                onClick={() => onSelectAvatar(line.avatarId)}
              >
                <span className="ticker-text">{line.text}</span>
                <time dateTime={line.ts}>{formatTimeAgo(line.ts)}</time>
              </button>
            </li>
          ))}
        </ol>
      )}
    </div>
  )
}
