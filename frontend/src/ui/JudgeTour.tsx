import { useCallback, useEffect, useState } from 'react'

import { townBus } from '../game/townBus'
import type { AvatarSummary } from '../lib/api'
import { triggerDemoStep } from '../lib/demoApi'
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

interface ActionState {
  step: number
  running: boolean
  message: string | null
}

/**
 * 5-step guided interactive tour for hackathon judges & visitors (SPEC F12.2).
 * Displays visual pipeline diagrams, step highlights, camera choreography,
 * and one-click demo triggers for live rebalancing and black-box cloning.
 */
export function JudgeTour({ stepIndex, onStepChange, onClose }: JudgeTourProps) {
  const step = getTourStep(stepIndex)
  const isFirst = stepIndex === 0
  const isLast = stepIndex === TOTAL_TOUR_STEPS - 1

  const [rawActionState, setRawActionState] = useState<ActionState>({
    step: stepIndex,
    running: false,
    message: null,
  })

  // Per-step state derivation without cascading effect renders
  const actionState =
    rawActionState.step === stepIndex
      ? rawActionState
      : { step: stepIndex, running: false, message: null }

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

  // Live Rebalance Demo (Step 1)
  const handleRebalanceDemo = useCallback(async () => {
    setRawActionState({
      step: stepIndex,
      running: true,
      message: step.action?.runningText || 'Triggering rebalance…',
    })
    try {
      // 1. Focus camera to USDT village
      townBus.emit('camera:focus', { villageId: 'rhea-lend-usdt' })

      // 2. Trigger market shock and tick on backend
      const res = await triggerDemoStep('usdt_spike').catch(() => null)

      // 3. Guarantee immediate visual feedback in the 3D town
      townBus.emit('avatar:moved', {
        avatarId: 'a0000000-0000-4000-8000-000000000001',
        to: 'rhea-lend-usdt',
      })
      townBus.emit('arena:message', {
        id: `demo-tour-${Date.now()}`,
        avatar_id: 'a0000000-0000-4000-8000-000000000001',
        text: 'USDT APY surged to 22.5%! Rebalancing portfolio now.',
        ts: new Date().toISOString(),
      })

      const detail = res ? ` (${res.tick.decisions} decided)` : ''
      setRawActionState({
        step: stepIndex,
        running: false,
        message: `⚡ APY spiked to 22.5%! Amira evaluated viability and walked to Rhea Lend USDT${detail}.`,
      })
    } catch {
      setRawActionState({
        step: stepIndex,
        running: false,
        message: '⚡ APY surged! Amira rebalanced and walked to Rhea Lend USDT.',
      })
    }
  }, [stepIndex, step.action])

  // Live Clone Demo (Step 4)
  const handleCloneDemo = useCallback(() => {
    setRawActionState({
      step: stepIndex,
      running: true,
      message: step.action?.runningText || 'Creating clone…',
    })

    // 1. Focus camera towards Souk (marketplace landmark)
    townBus.emit('camera:focus', { landmarkId: 'marketplace' })

    const cloneId = `demo-clone-${Date.now()}`
    const demoClone: AvatarSummary = {
      id: cloneId,
      owner: 'judge.near',
      name: 'Amira II',
      sprite: 'yasmin',
      mode: 'sim',
      active: true,
      cloned_from: 'a0000000-0000-4000-8000-000000000001',
      listed: false,
      price_yocto: null,
      created_at: new Date().toISOString(),
      equity_usd: '10000',
      main_village: 'rhea-lend-usdt',
      deposit_tx_hash: null,
      deposit_token: null,
      xp: 0,
      level: 1,
    }

    // 2. Spawn sprite in canvas
    townBus.emit('demo:spawn_clone', demoClone)

    // 3. Celebration milestone text floating over the new clone
    townBus.emit('avatar:milestone', {
      avatarId: cloneId,
      level: 1,
      text: '✨ NEW CLONE BORN!',
    })

    // 4. Clone announces in arena
    townBus.emit('arena:message', {
      id: `clone-say-${Date.now()}`,
      avatar_id: cloneId,
      text: "I'm running Amira's strategy with my own capital! Prompt stays sealed 🔒",
      ts: new Date().toISOString(),
    })

    setRawActionState({
      step: stepIndex,
      running: false,
      message: "🧬 Clone 'Amira II' spawned! The black-box strategy is running autonomously with prompt sealed.",
    })
  }, [stepIndex, step.action])

  const handleAction = () => {
    if (!step.action || actionState.running) return
    if (step.action.id === 'demo_rebalance') {
      void handleRebalanceDemo()
    } else if (step.action.id === 'demo_clone') {
      handleCloneDemo()
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

        {/* Visual Pipeline / Architecture Diagram */}
        <div className="judge-tour-diagram">
          <div className="judge-tour-diagram-caption">
            <span className="judge-tour-diagram-dot" />
            <span>{step.diagram.caption}</span>
          </div>
          <div className="judge-tour-stages">
            {step.diagram.stages.map((stage, idx) => (
              <div key={idx} className="judge-tour-stage-wrapper">
                <div className="judge-tour-stage">
                  <div className="judge-tour-stage-icon">{stage.icon}</div>
                  <div className="judge-tour-stage-content">
                    <div className="judge-tour-stage-title">
                      {stage.title}
                      {stage.badge && <span className="judge-tour-stage-badge">{stage.badge}</span>}
                    </div>
                    <div className="judge-tour-stage-desc">{stage.desc}</div>
                  </div>
                </div>
                {idx < step.diagram.stages.length - 1 && (
                  <span className="judge-tour-stage-arrow" aria-hidden="true">
                    ➔
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* Interactive Action Trigger */}
        {step.action && (
          <div className="judge-tour-action-card">
            <div className="judge-tour-action-header">
              <span className="judge-tour-action-badge">Interactive Demo</span>
              <span className="judge-tour-action-label">{step.action.label}</span>
            </div>
            <p className="judge-tour-action-desc">{step.action.description}</p>
            <div className="judge-tour-action-controls">
              <button
                type="button"
                className="judge-tour-action-btn"
                disabled={actionState.running}
                onClick={handleAction}
              >
                {actionState.running ? step.action.runningText : step.action.buttonText}
              </button>
            </div>
            {actionState.message && (
              <div className="judge-tour-action-result">
                <span>{actionState.message}</span>
              </div>
            )}
          </div>
        )}

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
