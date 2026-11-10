import { describe, expect, it } from 'vitest'

import {
  getTourStep,
  nextStepIndex,
  prevStepIndex,
  TOTAL_TOUR_STEPS,
  TOUR_STEPS,
} from './judgeTourModel'

describe('judgeTourModel', () => {
  it('defines exactly 5 steps covering SPEC F12.2 requirements', () => {
    expect(TOTAL_TOUR_STEPS).toBe(5)
    expect(TOUR_STEPS.map((s) => s.id)).toEqual([
      'town',
      'avatar',
      'leaderboard',
      'marketplace',
      'vault',
    ])
  })

  it('numbers steps sequentially from 1 to 5', () => {
    TOUR_STEPS.forEach((step, idx) => {
      expect(step.stepNumber).toBe(idx + 1)
      expect(step.title.length).toBeGreaterThan(0)
      expect(step.tag.length).toBeGreaterThan(0)
      expect(step.summary.length).toBeGreaterThan(0)
      expect(step.highlights.length).toBe(3)
      expect(step.diagram.stages.length).toBe(4)
      expect(step.diagram.caption.length).toBeGreaterThan(0)
    })
  })

  it('safely clamps getTourStep index', () => {
    expect(getTourStep(-5).id).toBe('town')
    expect(getTourStep(0).id).toBe('town')
    expect(getTourStep(2).id).toBe('leaderboard')
    expect(getTourStep(4).id).toBe('vault')
    expect(getTourStep(99).id).toBe('vault')
  })

  it('handles forward and backward step transitions correctly', () => {
    expect(nextStepIndex(0)).toBe(1)
    expect(nextStepIndex(1)).toBe(2)
    expect(nextStepIndex(3)).toBe(4)
    expect(nextStepIndex(4)).toBeNull()

    expect(prevStepIndex(0)).toBeNull()
    expect(prevStepIndex(1)).toBe(0)
    expect(prevStepIndex(4)).toBe(3)
  })

  it('targets expected UI panels for each step', () => {
    // Step 1: Town - no side panel
    expect(TOUR_STEPS[0].target.panel).toBe('none')
    expect(TOUR_STEPS[0].target.selectAvatar).toBe(false)
    expect(TOUR_STEPS[0].action?.id).toBe('demo_rebalance')

    // Step 2: Character panel - opens avatar selection
    expect(TOUR_STEPS[1].target.panel).toBe('none')
    expect(TOUR_STEPS[1].target.selectAvatar).toBe(true)

    // Step 3: Leaderboard panel
    expect(TOUR_STEPS[2].target.panel).toBe('leaderboard')

    // Step 4: Marketplace panel with clone action
    expect(TOUR_STEPS[3].target.panel).toBe('marketplace')
    expect(TOUR_STEPS[3].action?.id).toBe('demo_clone')

    // Step 5: Vault safety summary
    expect(TOUR_STEPS[4].target.panel).toBe('none')
  })
})
