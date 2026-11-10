import { useEffect, useState } from 'react'

import { faceUrl } from '../game/sprites'
import {
  confirmDeposit,
  getAvatar,
  getDecisions,
  getEquity,
  updateAvatar,
  type AvatarDetail,
  type Decision,
  type EquityPoint,
} from '../lib/api'
import { actionLabel, allocationShares, formatUsd } from '../lib/format'
import { currentAccount, depositToVault, withdrawFromVault } from '../lib/wallet'
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

/** How many of the 3 risk pips are filled. */
const RISK_PIPS: Record<AvatarDetail['risk'], number> = { low: 1, medium: 2, high: 3 }

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
  const [withdrawing, setWithdrawing] = useState(false)
  const [withdrawStatus, setWithdrawStatus] = useState<string | null>(null)
  const [depositing, setDepositing] = useState(false)
  const [depositStatus, setDepositStatus] = useState<string | null>(null)

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

  const handleDeposit = async () => {
    setDepositing(true)
    setDepositStatus(null)
    try {
      const amountUnits = (Number(avatar.starting_capital_usd) * 1_000_000).toString()
      const txHash = await depositToVault(avatar.id, amountUnits)
      const updated = await confirmDeposit(avatar.id, txHash)
      setData({ ...data, avatar: updated })
      setDepositStatus('Deposit confirmed! Avatar is now active.')
    } catch (err) {
      console.error('deposit failed', err)
      setDepositStatus('Deposit failed or cancelled.')
    } finally {
      setDepositing(false)
    }
  }

  const handleWithdraw = async () => {
    if (!window.confirm(`Withdraw funds from vault for ${avatar.name}?`)) return
    setWithdrawing(true)
    setWithdrawStatus(null)
    try {
      const txHash = await withdrawFromVault(avatar.id)
      setWithdrawStatus(`Withdrawn! Tx: ${txHash.slice(0, 8)}…`)
      const updated = await getAvatar(avatar.id)
      setData({ ...data, avatar: updated })
    } catch (err) {
      console.error('withdraw failed', err)
      setWithdrawStatus('Withdraw failed or cancelled.')
    } finally {
      setWithdrawing(false)
    }
  }

  const change = start > 0 ? (Number(avatar.equity_usd) / start - 1) * 100 : 0
  const positions = Object.entries(avatar.positions).sort(([, a], [, b]) => Number(b) - Number(a))
  const allocation = allocationShares(avatar.positions, avatar.cash_usd)

  return (
    <>
      <div className="panel-head">
        <img className="panel-face" src={faceUrl(avatar.sprite)} alt="" width={52} height={52} />
        <div>
          <h2>{avatar.name}</h2>
          <p className="panel-muted">
            owner <span className="mono">{avatar.owner}</span>
          </p>
        </div>
        <span className={`badge badge-${avatar.mode}`}>{avatar.mode}</span>
      </div>
      <p className="sheet-traits">
        <span className="risk-pips" title={`risk ${avatar.risk}`}>
          risk
          {[1, 2, 3].map((pip) => (
            <span key={pip} className={pip <= RISK_PIPS[avatar.risk] ? 'pip on' : 'pip'} />
          ))}
          {avatar.risk}
        </span>
        <span className="trait-chip">{GOAL_LABELS[avatar.goal]}</span>
      </p>
      <p className="panel-muted">
        {[
          avatar.cloned_from && 'clone',
          !avatar.active && 'paused',
          avatar.mode === 'real' && !avatar.deposit_tx_hash && 'awaiting deposit',
        ]
          .filter(Boolean)
          .join(' · ')}
      </p>
      {avatar.deposit_tx_hash && (
        <p className="panel-muted">
          vault deposit:{' '}
          <a
            href={`https://testnet.nearblocks.io/txns/${avatar.deposit_tx_hash}`}
            target="_blank"
            rel="noreferrer"
            className="mono"
            style={{ textDecoration: 'underline' }}
          >
            {avatar.deposit_tx_hash.slice(0, 8)}…{avatar.deposit_tx_hash.slice(-6)}
          </a>
        </p>
      )}
      {isOwner && (
        <div className="form-actions" style={{ flexWrap: 'wrap', gap: '0.5rem' }}>
          {avatar.mode === 'real' && !avatar.deposit_tx_hash ? (
            <button type="button" onClick={handleDeposit} disabled={depositing}>
              {depositing ? 'Depositing…' : `Deposit ${avatar.starting_capital_usd} tUSDC to Activate`}
            </button>
          ) : (
            <button type="button" onClick={toggleActive} disabled={toggling}>
              {toggling ? 'Saving…' : avatar.active ? 'Pause' : 'Start'}
            </button>
          )}

          {avatar.mode === 'real' && avatar.deposit_tx_hash && (
            <button
              type="button"
              className="secondary"
              onClick={handleWithdraw}
              disabled={withdrawing || toggling}
              title="Withdraw all funds from vault back to owner wallet"
            >
              {withdrawing ? 'Withdrawing…' : 'Withdraw funds'}
            </button>
          )}

          {toggleError && <span className="header-error">{toggleError}</span>}
          {depositStatus && <span className="header-note">{depositStatus}</span>}
          {withdrawStatus && <span className="header-note">{withdrawStatus}</span>}
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
        {allocation.length > 0 && (
          <div className="allocation-bar" role="img" aria-label="How the money is split">
            {allocation.map((share, index) => (
              <span
                key={share.key}
                className={share.key === 'cash' ? 'slice slice-cash' : `slice slice-${index % 4}`}
                style={{ width: `${share.percent}%` }}
                title={`${share.key} ${share.percent.toFixed(0)}%`}
              />
            ))}
          </div>
        )}
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
