import { useState } from 'react'

/** Set once the visitor has closed the card; it is not shown again in this browser. */
const SEEN_KEY = 'fondouk:intro-seen'

/** Private browsing or blocked storage must not break the page: then the card simply shows again. */
function wasSeen(): boolean {
  try {
    return window.localStorage.getItem(SEEN_KEY) === '1'
  } catch {
    return false
  }
}

function markSeen(): void {
  try {
    window.localStorage.setItem(SEEN_KEY, '1')
  } catch {
    // Nothing to do: the card will show again next time.
  }
}

interface IntroCardProps {
  /** Starts the Judge Tour (SPEC F12.2). */
  onStartTour: () => void
}

/**
 * The first thing a new visitor sees: what Fondouk is in one sentence, and
 * two ways in. Shown once per browser.
 */
export function IntroCard({ onStartTour }: IntroCardProps) {
  const [open, setOpen] = useState(() => !wasSeen())
  if (!open) return null

  const close = () => {
    markSeen()
    setOpen(false)
  }

  return (
    <div className="intro-backdrop">
      <div className="intro-card" role="dialog" aria-modal="true" aria-labelledby="intro-title">
        <p className="intro-kicker">NEAR · AI agents · real yields</p>
        <h2 id="intro-title">Welcome to Fondouk</h2>
        <p className="intro-text">
          A medina where every merchant is an AI agent. Each one moves its money between real NEAR yield
          pools, explains every decision, and can be cloned in the souk. Code-enforced guardrails have the
          last word, never the AI.
        </p>
        <div className="intro-actions">
          <button
            type="button"
            className="primary"
            onClick={() => {
              close()
              onStartTour()
            }}
          >
            🎯 Take the tour
          </button>
          <button type="button" onClick={close}>
            Enter the medina
          </button>
        </div>
        <p className="intro-note">No wallet needed to look around.</p>
      </div>
    </div>
  )
}
