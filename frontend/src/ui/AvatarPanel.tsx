import { useEffect, useState } from 'react'

import { faceUrl } from '../game/sprites'
import {
  getAvatar,
  getDecisions,
  getEquity,
  updateAvatar,
  type AvatarDetail,
  type Decision,
  type EquityPoint,
} from '../lib/api'
import { actionLabel, formatUsd } from '../lib/format'
import { currentAccount } from '../lib/wallet'
import { LineChart } from './LineChart'

interface AvatarPanelProps {
  avatarId: string
  /** Changes when a live `decision` arrives for this character: reload. */
  refreshKey: number
}

interface AvatarData {
  avatar: AvatarDetail
  decisions: Decision[]
  equity: EquityPoint[]
  /** The logged-in account, `null` when nobody is: decides the Start/Pause button. */
  viewer: string | null
}

const GOAL_LABELS: Record<AvatarDetail['goal'], string> = {
  steady_income: 'steady income',
  max_yield: 'max yield',
  protect_capital: 'protect capital',
}

/**
 * F8.3: owner, mode badge, equity chart, last 10 decisions with their public
 * reasons, current positions.
 *
 * History is read for the character's own mode only (`sim` or `real`) and
 * labelled with it: sources are never mixed (CLAUDE.md "data source labels").
 * The persona is never shown here, even to its creator (D7).
 *
 * The owner gets a Start / Pause button (`PATCH {active}`, D63): a new
 * character is created paused.
 */
export function AvatarPanel({ avatarId, refreshKey }: AvatarPanelProps) {
  const [data, setData] = useState<AvatarData | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [toggling, setToggling] = useState(false)
  const [toggleError, setToggleError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      // The mode decides which history to read, so the detail comes first.
      const avatar = await getAvatar(avatarId)
      const [decisions, equity, viewer] = await Promise.all([
        getDecisions(avatarId, avatar.mode, 10),
        getEquity(avatarId, avatar.mode),
        currentAccount(),
      ])
      if (!cancelled) {
        setData({ avatar, decisions, equity, viewer })
        setError(null)
      }
    }
    load().catch((err) => {
      console.error('loading the character failed', err)
      if (!cancelled) setError('Could not load this character.')
    })
    return () => {
      cancelled = true
    }
  }, [avatarId, refreshKey])

  if (error) return <p className="panel-error">{error}</p>
  // A different character was clicked: show "loading" instead of old data.
  if (!data || data.avatar.id !== avatarId) return <p className="panel-muted">Loading…</p>

  const { avatar, decisions, equity, viewer } = data
  const isOwner = viewer !== null && viewer === avatar.owner
  const start = Number(avatar.starting_capital_usd)

  // The backend checks ownership again (403 for anyone else).
  const toggleActive = async () => {
    setToggling(true)
    setToggleError(null)
    try {
      const updated = await updateAvatar(avatar.id, { active: !avatar.active })
      setData({ ...data, avatar: updated })
    } catch (err) {
      console.error('changing start/pause failed', err)
      setToggleError('Could not change it. Try again.')
    } finally {
      setToggling(false)
    }
  }
  const change = start > 0 ? (Number(avatar.equity_usd) / start - 1) * 100 : 0
  const positions = Object.entries(avatar.positions).sort(([, a], [, b]) => Number(b) - Number(a))

  return (
    <>
      <div className="panel-head">
        <img className="panel-face" src={faceUrl(avatar.sprite)} alt="" width={38} height={38} />
        <div>
          <h2>{avatar.name}</h2>
          <p className="panel-muted">
            owner <span className="mono">{avatar.owner}</span>
          </p>
        </div>
        <span className={`badge badge-${avatar.mode}`}>{avatar.mode}</span>
      </div>
      <p className="panel-muted">
        risk {avatar.risk} · goal {GOAL_LABELS[avatar.goal]}
        {avatar.cloned_from && ' · clone'}
        {!avatar.active && ' · paused'}
      </p>
      {isOwner && (
        <div className="form-actions">
          <button type="button" onClick={toggleActive} disabled={toggling}>
            {toggling ? 'Saving…' : avatar.active ? 'Pause' : 'Start'}
          </button>
          {toggleError && <span className="header-error">{toggleError}</span>}
        </div>
      )}

      <section>
        <h3>
          Equity <span className={`badge badge-${avatar.mode}`}>{avatar.mode}</span>
        </h3>
        <p className="panel-big">
          {formatUsd(avatar.equity_usd)}{' '}
          <span className={change >= 0 ? 'up' : 'down'}>
            {change >= 0 ? '+' : ''}
            {change.toFixed(2)}%
          </span>
        </p>
        <LineChart
          values={equity.map((point) => Number(point.equity_usd))}
          format={(value) => formatUsd(String(value))}
          emptyText={`No ${avatar.mode} history yet: it starts at the first tick.`}
          label={`Equity, ${avatar.mode}`}
        />
      </section>

      <section>
        <h3>Positions</h3>
        <ul className="panel-list">
          {positions.map(([village, usd]) => (
            <li key={village}>
              <span className="mono">{village}</span>
              <span>{formatUsd(usd)}</span>
            </li>
          ))}
          <li>
            <span>cash</span>
            <span>{formatUsd(avatar.cash_usd)}</span>
          </li>
        </ul>
      </section>

      <section>
        <h3>Last decisions</h3>
        {decisions.length === 0 && <p className="panel-muted">No decisions yet.</p>}
        <ol className="decisions">
          {decisions.map((decision) => (
            <li key={decision.id}>
              <div className="decision-top">
                <span className="mono">{actionLabel(decision.applied_action)}</span>
                <time dateTime={decision.ts}>{new Date(decision.ts).toLocaleString()}</time>
              </div>
              {!decision.valid && (
                <p className="decision-rejected">
                  refused by guardrail <span className="mono">{decision.rejected_rule ?? '?'}</span>
                </p>
              )}
              {decision.reason_public && <p>{decision.reason_public}</p>}
              {Number(decision.cost_usd) > 0 && (
                <p className="panel-muted">cost {formatUsd(decision.cost_usd)}</p>
              )}
            </li>
          ))}
        </ol>
      </section>
    </>
  )
}
