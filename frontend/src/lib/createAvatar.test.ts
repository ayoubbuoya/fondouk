import { describe, expect, it } from 'vitest'

import {
  toCreateRequest,
  validateCreateForm,
  validateDetails,
  type CreateAvatarForm,
} from './createAvatar'

const valid: CreateAvatarForm = {
  name: '  Hanna  ',
  sprite: 'boy',
  persona: 'Prefer lending. Never put more than half in one pool.',
  risk: 'medium',
  goal: 'steady_income',
  starting_capital_usd: 10_000,
  mode: 'sim',
}

describe('validateCreateForm', () => {
  it('accepts a complete form', () => {
    expect(validateCreateForm(valid)).toBeNull()
  })

  it('refuses an empty or too long name', () => {
    expect(validateCreateForm({ ...valid, name: '   ' })).toMatch(/name/)
    expect(validateCreateForm({ ...valid, name: 'x'.repeat(33) })).toMatch(/32/)
    expect(validateCreateForm({ ...valid, name: 'x'.repeat(32) })).toBeNull()
  })

  it('refuses an empty or too long strategy', () => {
    expect(validateCreateForm({ ...valid, persona: ' \n ' })).toMatch(/strategy/)
    expect(validateCreateForm({ ...valid, persona: 'a'.repeat(2001) })).toMatch(/2000/)
  })

  it('counts characters, not UTF-16 units, like the backend', () => {
    // An emoji is 2 UTF-16 units but 1 character: 2000 of them are allowed.
    expect(validateCreateForm({ ...valid, persona: '😀'.repeat(2000) })).toBeNull()
  })

  it('refuses a starting capital the backend does not offer', () => {
    expect(validateCreateForm({ ...valid, starting_capital_usd: 5_000 })).toMatch(/capital/)
  })

  it('validates real mode deposit options', () => {
    expect(validateCreateForm({ ...valid, mode: 'real', starting_capital_usd: 50 })).toBeNull()
    expect(validateCreateForm({ ...valid, mode: 'real', starting_capital_usd: 10_000 })).toMatch(/tUSDC/)
  })
})

describe('validateDetails', () => {
  it('ignores the strategy, which step 2 asks for', () => {
    expect(validateDetails({ ...valid, persona: '' })).toBeNull()
    expect(validateDetails({ ...valid, name: '' })).toMatch(/name/)
  })

  it('validates real mode capital options', () => {
    expect(validateDetails({ ...valid, mode: 'real', starting_capital_usd: 10 })).toBeNull()
    expect(validateDetails({ ...valid, mode: 'real', starting_capital_usd: 500 })).toMatch(/tUSDC/)
  })
})

describe('toCreateRequest', () => {
  it('trims the name and preserves mode', () => {
    expect(toCreateRequest(valid)).toEqual({ ...valid, name: 'Hanna', mode: 'sim' })
    expect(toCreateRequest({ ...valid, mode: 'real', starting_capital_usd: 50 })).toEqual({
      ...valid,
      name: 'Hanna',
      mode: 'real',
      starting_capital_usd: 50,
    })
  })
})
