import { useState } from 'react'

import { SPRITES, faceUrl } from '../game/sprites'
import {
  ApiError,
  confirmDeposit,
  createAvatar,
  getDecisions,
  updateAvatar,
  type AvatarDetail,
  type CreatedAvatar,
  type Decision,
} from '../lib/api'
import {
  GOALS,
  MAX_NAME_CHARS,
  MAX_PERSONA_CHARS,
  REAL_STARTING_CAPITALS,
  RISKS,
  STARTING_CAPITALS,
  toCreateRequest,
  validateCreateForm,
  validateDetails,
  type CreateAvatarForm,
} from '../lib/createAvatar'
import { actionLabel, formatUsd, parseNearToYocto } from '../lib/format'
import { depositToVault } from '../lib/wallet'
import { LineChart } from './LineChart'

interface CreateAvatarPanelProps {
  /** Called after Start or "Keep paused": open the new character's panel. */
  onDone: (avatarId: string, started: boolean) => void
}

type Step = 'details' | 'strategy' | 'testing' | 'result'

interface Result {
  created: CreatedAvatar
  /** The backtest decisions, oldest first (same order as the chart). */
  decisions: Decision[]
}

const GOAL_LABELS: Record<AvatarDetail['goal'], string> = {
  steady_income: 'steady income',
  max_yield: 'max yield',
  protect_capital: 'protect capital',
}

const PERSONA_PLACEHOLDER = `Example:
You are a careful saver. Prefer lending villages over DEX pools.
Never keep more than half of the money in one village.
Only move when the new village pays at least 1% more.`

const EMPTY_FORM: CreateAvatarForm = {
  name: '',
  sprite: SPRITES[0],
  persona: '',
  risk: 'medium',
  goal: 'steady_income',
  starting_capital_usd: 10_000,
  mode: 'sim',
  listed: true,
  price_yocto: '500000000000000000000000',
}

/**
 * The create-avatar wizard (F3, D63), shown only to a logged-in user:
 *
 * ```text
 * details -> strategy -> testing (POST /avatars, ~7 AI calls) -> result -> Start
 * ```
 *
 * The character is created **paused** with its backtest; nothing runs live
 * until the owner presses Start (`PATCH {active: true}`). The backtest is
 * labelled as past data, never mixed with live results.
 */
export function CreateAvatarPanel({ onDone }: CreateAvatarPanelProps) {
  const [step, setStep] = useState<Step>('details')
  const [form, setForm] = useState<CreateAvatarForm>(EMPTY_FORM)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<Result | null>(null)
  const [starting, setStarting] = useState(false)
  const [priceNear, setPriceNear] = useState('0.5')

  const update = (changes: Partial<CreateAvatarForm>) => setForm((old) => ({ ...old, ...changes }))

  const handlePriceNearChange = (val: string) => {
    setPriceNear(val)
    const yocto = parseNearToYocto(val)
    update({ price_yocto: yocto ?? undefined })
  }

  const goToStrategy = () => {
    const problem = validateDetails(form)
    setError(problem)
    if (!problem) setStep('strategy')
  }

  const runBacktest = async () => {
    const problem = validateCreateForm(form)
    setError(problem)
    if (problem) return
    setStep('testing')
    try {
      const created = await createAvatar(toCreateRequest(form))
      // Newest first from the API: reversed to read day by day like the chart.
      const decisions =
        created.backtest === null
          ? []
          : (await getDecisions(created.avatar.id, 'backtest', 7)).reverse()
      setResult({ created, decisions })
      setStep('result')
    } catch (err) {
      console.error('creating the character failed', err)
      setError(createErrorMessage(err))
      setStep('strategy')
    }
  }

  const finish = async (start: boolean) => {
    if (!result) return
    const id = result.created.avatar.id
    if (!start) {
      onDone(id, false)
      return
    }
    setStarting(true)
    setError(null)
    try {
      if (form.mode === 'real') {
        const amountUnits = (form.starting_capital_usd * 1_000_000).toString()
        const txHash = await depositToVault(id, amountUnits)
        await confirmDeposit(id, txHash)
      } else {
        await updateAvatar(id, { active: true })
      }
      onDone(id, true)
    } catch (err) {
      console.error('starting or depositing for the character failed', err)
      if (form.mode === 'real') {
        setError(
          'Vault deposit or confirmation failed. Your avatar was created paused; you can deposit and activate it later from its panel.',
        )
      } else {
        setError('Could not start it. It stays paused; you can start it from its panel.')
      }
      setStarting(false)
    }
  }

  return (
    <>
      <h2>Create a character</h2>
      <p className="panel-muted">
        Step {stepNumber(step)} of 3 · {form.mode === 'real' ? 'real mode (tUSDC vault)' : 'simulation mode (paper money)'}
      </p>
      {error && <p className="notice notice-error">{error}</p>}

      {step === 'details' && (
        <div className="form">
          <label>
            Name
            <input
              value={form.name}
              maxLength={MAX_NAME_CHARS}
              onChange={(event) => update({ name: event.target.value })}
            />
          </label>

          <fieldset>
            <legend>Look</legend>
            <div className="sprite-picker">
              {SPRITES.map((sprite) => (
                <button
                  key={sprite}
                  type="button"
                  className={sprite === form.sprite ? 'sprite-option selected' : 'sprite-option'}
                  aria-pressed={sprite === form.sprite}
                  aria-label={sprite.replace('_', ' ')}
                  onClick={() => update({ sprite })}
                >
                  <img src={faceUrl(sprite)} alt="" width={38} height={38} />
                </button>
              ))}
            </div>
          </fieldset>

          <label>
            Mode
            <div style={{ display: 'flex', gap: '1.5rem', marginTop: '0.35rem', marginBottom: '0.5rem' }}>
              <label style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', cursor: 'pointer' }}>
                <input
                  type="radio"
                  name="mode"
                  value="sim"
                  checked={form.mode === 'sim'}
                  onChange={() => update({ mode: 'sim', starting_capital_usd: 10_000 })}
                />
                Simulation (paper money)
              </label>
              <label style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', cursor: 'pointer' }}>
                <input
                  type="radio"
                  name="mode"
                  value="real"
                  checked={form.mode === 'real'}
                  onChange={() => update({ mode: 'real', starting_capital_usd: 50 })}
                />
                Real Mode (on-chain Vault)
              </label>
            </div>
          </label>

          <label>
            Risk level
            <select
              value={form.risk}
              onChange={(event) => update({ risk: event.target.value as CreateAvatarForm['risk'] })}
            >
              {RISKS.map((risk) => (
                <option key={risk} value={risk}>
                  {risk}
                </option>
              ))}
            </select>
          </label>
          <p className="panel-muted">
            The risk level sets hard limits in code (how much may move, how often, how much in one
            village). The AI cannot change them.
          </p>

          <label>
            Goal
            <select
              value={form.goal}
              onChange={(event) => update({ goal: event.target.value as CreateAvatarForm['goal'] })}
            >
              {GOALS.map((goal) => (
                <option key={goal} value={goal}>
                  {GOAL_LABELS[goal]}
                </option>
              ))}
            </select>
          </label>

          {form.mode === 'real' ? (
            <>
              <label>
                Deposit amount (tUSDC)
                <select
                  value={form.starting_capital_usd}
                  onChange={(event) => update({ starting_capital_usd: Number(event.target.value) })}
                >
                  {REAL_STARTING_CAPITALS.map((usd) => (
                    <option key={usd} value={usd}>
                      {usd} tUSDC
                    </option>
                  ))}
                </select>
              </label>
              <p className="panel-muted">
                Real on-chain deposit to the Fondouk Vault contract on NEAR testnet. Need test tokens?
                Claim 100 tUSDC from the faucet in the header above.
              </p>
            </>
          ) : (
            <label>
              Starting capital (paper)
              <select
                value={form.starting_capital_usd}
                onChange={(event) => update({ starting_capital_usd: Number(event.target.value) })}
              >
                {STARTING_CAPITALS.map((usd) => (
                  <option key={usd} value={usd}>
                    {formatUsd(String(usd))}
                  </option>
                ))}
              </select>
            </label>
          )}

          <div
            style={{
              marginTop: '0.25rem',
              padding: '10px 12px',
              background: 'var(--panel-raised)',
              borderRadius: '6px',
              border: '1px solid var(--border)',
            }}
          >
            <label style={{ display: 'flex', alignItems: 'center', gap: '8px', cursor: 'pointer', margin: 0 }}>
              <input
                type="checkbox"
                checked={form.listed ?? true}
                onChange={(event) => update({ listed: event.target.checked })}
              />
              <span>Allow others to clone this character in Souk</span>
            </label>
            {(form.listed ?? true) && (
              <div style={{ marginTop: '8px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                <span style={{ fontSize: '12px', color: 'var(--muted)' }}>Clone price:</span>
                <input
                  type="number"
                  step="0.1"
                  min="0.01"
                  value={priceNear}
                  onChange={(event) => handlePriceNearChange(event.target.value)}
                  style={{ width: '80px', padding: '3px 6px', fontSize: '12px' }}
                />
                <span style={{ fontSize: '12px' }}>NEAR</span>
                <span className="panel-muted" style={{ fontSize: '11px', marginLeft: 'auto' }}>
                  (you receive 90%)
                </span>
              </div>
            )}
          </div>

          <div className="form-actions">
            <button type="button" onClick={goToStrategy}>
              Next: strategy
            </button>
          </div>
        </div>
      )}

      {step === 'strategy' && (
        <div className="form">
          <label>
            Strategy: persona and rules
            <textarea
              rows={10}
              value={form.persona}
              placeholder={PERSONA_PLACEHOLDER}
              onChange={(event) => update({ persona: event.target.value })}
            />
          </label>
          <p className="panel-muted">
            {[...form.persona].length} / {MAX_PERSONA_CHARS} · private: only you can read it, buyers
            of a clone never see it.
          </p>
          <div className="form-actions">
            <button type="button" className="secondary" onClick={() => setStep('details')}>
              Back
            </button>
            <button type="button" onClick={runBacktest}>
              Create and test on the last 7 days
            </button>
          </div>
        </div>
      )}

      {step === 'testing' && (
        <p className="notice">
          Testing your strategy on the last 7 days of real yields: one AI decision per day. This
          can take a minute or more, please keep this panel open.
        </p>
      )}

      {step === 'result' && result && (
        <BacktestResult
          result={result}
          starting={starting}
          onStart={() => finish(true)}
          onKeepPaused={() => finish(false)}
        />
      )}
    </>
  )
}

interface BacktestResultProps {
  result: Result
  starting: boolean
  onStart: () => void
  onKeepPaused: () => void
}

/** Step 3: the backtest chart and its decisions, then Start. */
function BacktestResult({ result, starting, onStart, onKeepPaused }: BacktestResultProps) {
  const { avatar, backtest } = result.created
  const first = Number(avatar.starting_capital_usd)
  const last = backtest && backtest.length > 0 ? Number(backtest[backtest.length - 1].equity_usd) : first
  const change = first > 0 ? (last / first - 1) * 100 : 0

  return (
    <>
      <p className="panel-muted">
        <strong>{avatar.name}</strong> is created and <strong>paused</strong>.
      </p>
      <section>
        <h3>
          Equity <span className="badge badge-backtest">backtest</span>
        </h3>
        {backtest === null ? (
          <p className="notice">
            Backtest unavailable (no AI configured on the server, or no yield history yet). You
            can still start the character.
          </p>
        ) : (
          <>
            <p className="panel-big">
              {formatUsd(String(first))} → {formatUsd(String(last))}{' '}
              <span className={change >= 0 ? 'up' : 'down'}>
                {change >= 0 ? '+' : ''}
                {change.toFixed(2)}%
              </span>
            </p>
            <LineChart
              values={backtest.map((point) => Number(point.equity_usd))}
              format={(value) => formatUsd(String(value))}
              emptyText="Not enough history for a chart."
              label="Equity, backtest, last 7 days"
            />
            <p className="panel-muted">
              Past 7 days, replayed with your strategy and the same rules and costs as live mode.
              Not a promise of future results.
            </p>
          </>
        )}
      </section>

      {result.decisions.length > 0 && (
        <section>
          <h3>What it decided each day</h3>
          <ol className="decisions">
            {result.decisions.map((decision) => (
              <li key={decision.id}>
                <div className="decision-top">
                  <span className="mono">{actionLabel(decision.applied_action)}</span>
                  <time dateTime={decision.ts}>{new Date(decision.ts).toLocaleDateString()}</time>
                </div>
                {!decision.valid && (
                  <p className="decision-rejected">
                    refused <span className="mono">{decision.rejected_rule ?? '?'}</span>
                  </p>
                )}
                {decision.reason_public && <p>{decision.reason_public}</p>}
              </li>
            ))}
          </ol>
        </section>
      )}

      <div className="form-actions">
        <button type="button" className="secondary" onClick={onKeepPaused} disabled={starting}>
          Keep paused
        </button>
        <button type="button" onClick={onStart} disabled={starting}>
          {starting
            ? avatar.mode === 'real'
              ? 'Depositing & Starting…'
              : 'Starting…'
            : avatar.mode === 'real'
              ? `Deposit ${avatar.starting_capital_usd} tUSDC & Start`
              : 'Start'}
        </button>
      </div>
    </>
  )
}

function stepNumber(step: Step): number {
  if (step === 'details') return 1
  if (step === 'strategy') return 2
  return 3
}

/** The backend's 400 text is written for users (limit, bad field); the rest stays generic. */
function createErrorMessage(err: unknown): string {
  if (err instanceof ApiError && err.status === 400) return err.message
  if (err instanceof ApiError && err.status === 401) return 'Your session ended. Connect your wallet again.'
  return 'Creating the character failed. If it appears in the town, it was created paused.'
}
