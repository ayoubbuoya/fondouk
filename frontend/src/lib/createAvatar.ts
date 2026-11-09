/**
 * The create-avatar form (F3.1, D63): its limits and its check.
 *
 * The limits repeat the backend's (`api/avatars.rs`). The backend checks again
 * and is the one that decides; this check only lets the user fix a mistake
 * before waiting a minute for the backtest.
 */

import type { CreateAvatarRequest } from './api'

export const MAX_NAME_CHARS = 32
export const MAX_PERSONA_CHARS = 2000
/** Sim starting capital options in USD (backend `STARTING_CAPITALS`). */
export const STARTING_CAPITALS = [1_000, 10_000, 100_000] as const
/** Real mode starting capital options in tUSDC / USD. */
export const REAL_STARTING_CAPITALS = [10, 25, 50, 100] as const
export const RISKS = ['low', 'medium', 'high'] as const
export const GOALS = ['steady_income', 'max_yield', 'protect_capital'] as const

export type CreateAvatarForm = CreateAvatarRequest

/**
 * Step 1 of the wizard (everything except the strategy). Returns the first
 * problem as a short sentence, or `null`. Lengths count characters (code
 * points), not UTF-16 units, like the backend.
 */
export function validateDetails(form: CreateAvatarForm): string | null {
  const name = form.name.trim()
  if (name.length === 0) return 'Give your character a name.'
  if ([...name].length > MAX_NAME_CHARS) return `The name is at most ${MAX_NAME_CHARS} characters.`
  if (form.mode === 'real') {
    if (!(REAL_STARTING_CAPITALS as readonly number[]).includes(form.starting_capital_usd)) {
      return 'Pick a deposit amount for real mode (10, 25, 50 or 100 tUSDC).'
    }
  } else {
    if (!(STARTING_CAPITALS as readonly number[]).includes(form.starting_capital_usd)) {
      return 'Pick a starting capital.'
    }
  }
  return null
}

/** The whole form, checked before `POST /avatars`. */
export function validateCreateForm(form: CreateAvatarForm): string | null {
  const detailsProblem = validateDetails(form)
  if (detailsProblem) return detailsProblem
  if (form.persona.trim().length === 0) return 'Write the strategy your character should follow.'
  if ([...form.persona].length > MAX_PERSONA_CHARS) {
    return `The strategy is at most ${MAX_PERSONA_CHARS} characters.`
  }
  return null
}

/** The request body: the trimmed name. */
export function toCreateRequest(form: CreateAvatarForm): CreateAvatarRequest {
  return { ...form, name: form.name.trim() }
}
