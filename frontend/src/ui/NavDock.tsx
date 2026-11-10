interface NavDockProps {
  /** Which entry is open now, to highlight it. */
  active: 'tour' | 'arena' | 'leaderboard' | 'marketplace' | null
  /** Starts the 5-step Judge Tour (SPEC F12.2). */
  onStartTour: () => void
  onOpenArena: () => void
  onOpenLeaderboard: () => void
  onOpenMarketplace: () => void
}

/**
 * The menu on the left edge of the town. Every entry is also a building on
 * the map (D69); the dock is the shortcut that needs no map knowledge.
 */
export function NavDock({ active, onStartTour, onOpenArena, onOpenLeaderboard, onOpenMarketplace }: NavDockProps) {
  const entries = [
    { id: 'tour', icon: '🎯', label: 'Tour', title: 'Guided 5-step overview for judges and visitors', onClick: onStartTour },
    { id: 'arena', icon: '💬', label: 'Arena', title: 'What the characters say in the court', onClick: onOpenArena },
    { id: 'leaderboard', icon: '🏆', label: 'Ranks', title: 'Leaderboard: returns, drawdown, clones', onClick: onOpenLeaderboard },
    { id: 'marketplace', icon: '🛒', label: 'Souk', title: 'Marketplace: buy a copy of a character', onClick: onOpenMarketplace },
  ] as const

  return (
    <nav className="dock" aria-label="Main menu">
      {entries.map((entry) => (
        <button
          key={entry.id}
          type="button"
          className={`dock-item${active === entry.id ? ' active' : ''}`}
          onClick={entry.onClick}
          title={entry.title}
        >
          <span className="dock-icon" aria-hidden="true">
            {entry.icon}
          </span>
          <span className="dock-label">{entry.label}</span>
        </button>
      ))}
    </nav>
  )
}
