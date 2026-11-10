import { useEffect } from 'react'

import {
  getTourStep,
  nextStepIndex,
  prevStepIndex,
  TOTAL_TOUR_STEPS,
  TOUR_STEPS,
} from './judgeTourModel'

interface JudgeTourProps {
  /** 0-based step index (0 to 4). */
  stepIndex: number
  /** Invoked when user navigates to another step. */
  onStepChange: (index: number) => void
  /** Invoked when user skips or finishes the tour. */
  onClose: () => void
}

/**
 * 5-step guided overlay for hackathon judges & visitors (SPEC F12.2).
 * Floats unobtrusively so the judge can inspect the active panels and town.
 */
export function JudgeTour({ stepIndex, onStepChange, onClose }: JudgeTourProps) {
  const step = getTourStep(stepIndex)
  const isFirst = stepIndex === 0
  const isLast = stepIndex === TOTAL_TOUR_STEPS - 1

  // Keyboard navigation: Escape closes, Left/Right arrows navigate
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose()
      } else if (e.key === 'ArrowRight') {
        const next = nextStepIndex(stepIndex)
        if (next !== null) onStepChange(next)
      } else if (e.key === 'ArrowLeft') {
        const prev = prevStepIndex(stepIndex)
        if (prev !== null) onStepChange(prev)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [stepIndex, onStepChange, onClose])

  const onNext = () => {
    const next = nextStepIndex(stepIndex)
    if (next !== null) {
      onStepChange(next)
    } else {
      onClose()
    }
  }

  const onPrev = () => {
    const prev = prevStepIndex(stepIndex)
    if (prev !== null) {
      onStepChange(prev)
    }
  }

  return (
    <div className="judge-tour-backdrop" role="dialog" aria-modal="true" aria-label="Judge Tour">
      <div className="judge-tour-card">
        {/* Header bar */}
        <div className="judge-tour-header">
          <div className="judge-tour-badges">
            <span className="judge-tour-step-pill">
              Step {step.stepNumber} of {TOTAL_TOUR_STEPS}
            </span>
            <span className="judge-tour-tag">{step.tag}</span>
          </div>
          <button
            type="button"
            className="judge-tour-close-btn"
            onClick={onClose}
            title="Exit Tour (Esc)"
            aria-label="Exit Tour"
          >
            ✕
          </button>
        </div>

        {/* Title and summary */}
        <h2 className="judge-tour-title">{step.title}</h2>
        <p className="judge-tour-summary">{step.summary}</p>

        {/* Highlights bullet list */}
        <div className="judge-tour-highlights">
          {step.highlights.map((highlight, idx) => (
            <div key={idx} className="judge-tour-highlight-item">
              <span className="judge-tour-bullet">◈</span>
              <span className="judge-tour-highlight-text">{highlight}</span>
            </div>
          ))}
        </div>

        {/* Footer with progress dots and navigation */}
        <div className="judge-tour-footer">
          <div className="judge-tour-dots" role="tablist" aria-label="Tour steps">
            {TOUR_STEPS.map((s, idx) => (
              <button
                key={s.id}
                type="button"
                className={`judge-tour-dot ${idx === stepIndex ? 'active' : ''}`}
                onClick={() => onStepChange(idx)}
                title={`Jump to step ${idx + 1}: ${s.title}`}
                aria-label={`Step ${idx + 1}: ${s.title}`}
              />
            ))}
          </div>

          <div className="judge-tour-actions">
            {!isFirst && (
              <button
                type="button"
                className="judge-tour-btn-secondary"
                onClick={onPrev}
              >
                ← Back
              </button>
            )}
            <button
              type="button"
              className="judge-tour-btn-primary"
              onClick={onNext}
            >
              {isLast ? 'Finish Tour 🎉' : 'Next →'}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
