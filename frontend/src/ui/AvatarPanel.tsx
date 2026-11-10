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
import { actionLabel, allocationShares, formatNear, formatUsd, parseNearToYocto } from '../lib/format'
import { badgeIcon, xpPercent } from '../lib/progression'
import { currentAccount, depositToVault, withdrawFromVault } from '../lib/wallet'
import { CloneModal } from './CloneModal'
import { LineChart } from './LineChart'

interface AvatarPanelProps {
  avatarId: string
  /** Changes when a live `decision` arrives for this character: reload. */
  refreshKey: number
  /** Called after a clone is created from this avatar. */
  onOpenAvatar?: (id: string) => void
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
 * reasons, current positions. D74: level, XP bar and the badge catalog, all
 * as sent by the backend (the browser computes no XP).
 *
 * History is read for the character's own mode only (`sim` or `real`) and
 * labelled with it: sources are never mixed (CLAUDE.md "data source labels").
 * The persona is never shown here, even to its creator (D7).
 *
 * The owner gets a Start / Pause button (`PATCH {active}`, D63): a new
 * character is created paused.
 */
export function AvatarPanel({ avatarId, refreshKey, onOpenAvatar }: AvatarPanelProps) {
  const [data, setData] = useState<AvatarData | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [toggling, setToggling] = useState(false)
  const [toggleError, setToggleError] = useState<string | null>(null)
  const [withdrawing, setWithdrawing] = useState(false)
  const [withdrawStatus, setWithdrawStatus] = useState<string | null>(null)
  const [depositing, setDepositing] = useState(false)
  const [depositStatus, setDepositStatus] = useState<string | null>(null)
  const [showCloneModal, setShowCloneModal] = useState(false)
  const [updatingListing, setUpdatingListing] = useState(false)
  const [listingError, setListingError] = useState<string | null>(null)
  const [editPriceNear, setEditPriceNear] = useState('0.5')

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
        if (avatar.price_yocto) {
          const formatted = formatNear(avatar.price_yocto).replace(' NEAR', '')
          if (formatted !== '–') setEditPriceNear(formatted)
        }
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

  const handleToggleListing = async (nextListed: boolean) => {
    setUpdatingListing(true)
    setListingError(null)
    try {
      let priceYocto: string | undefined
      if (nextListed) {
        priceYocto = parseNearToYocto(editPriceNear) ?? '500000000000000000000000'
      }
      const updated = await updateAvatar(avatar.id, {
        listed: nextListed,
        price_yocto: priceYocto,
      })
      setData({ ...data, avatar: updated })
    } catch (err) {
      console.error('updating listing failed', err)
      setListingError('Could not update marketplace listing.')
    } finally {
      setUpdatingListing(false)
    }
  }

  const handleSavePrice = async () => {
    const yocto = parseNearToYocto(editPriceNear)
    if (!yocto || BigInt(yocto) <= 0n) {
      setListingError('Enter a valid price greater than 0 NEAR.')
      return
    }
    setUpdatingListing(true)
    setListingError(null)
    try {
      const updated = await updateAvatar(avatar.id, {
        listed: true,
        price_yocto: yocto,
      })
      setData({ ...data, avatar: updated })
    } catch (err) {
      console.error('saving price failed', err)
      setListingError('Could not update price.')
    } finally {
      setUpdatingListing(false)
    }
  }

  const change = start > 0 ? (Number(avatar.equity_usd) / start - 1) * 100 : 0
  const positions = Object.entries(avatar.positions).sort(([, a], [, b]) => Number(b) - Number(a))
  const allocation = allocationShares(avatar.positions, avatar.cash_usd)
  const unlockedBadges = avatar.badges.filter((badge) => badge.unlocked_at !== null).length

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
      {/* Level and XP (D74): experience from activity, not a performance number. */}
      <div className="xp-row">
        <span className="level-chip">Lv {avatar.level}</span>
        <div
          className="xp-bar"
          role="progressbar"
          aria-label="Experience in this level"
          aria-valuemin={0}
          aria-valuemax={avatar.xp_for_next_level}
          aria-valuenow={avatar.xp_into_level}
        >
          <span style={{ width: `${xpPercent(avatar.xp_into_level, avatar.xp_for_next_level)}%` }} />
        </div>
        <span className="xp-text">
          {avatar.xp_into_level} / {avatar.xp_for_next_level} XP
        </span>
      </div>
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

      {!isOwner && avatar.listed && (
        <div style={{ marginTop: '12px', marginBottom: '8px' }}>
          <button
            type="button"
            className="btn-primary"
            style={{ width: '100%', padding: '8px 14px', fontSize: '13px' }}
            disabled={viewer === null}
            title={viewer === null ? 'Connect wallet to clone this character' : undefined}
            onClick={() => setShowCloneModal(true)}
          >
            {avatar.price_yocto === '0' || !avatar.price_yocto
              ? 'Clone Strategy (Free)'
              : `Clone Strategy (${formatNear(avatar.price_yocto)})`}
          </button>
          {viewer === null && (
            <p className="panel-muted" style={{ fontSize: '11px', marginTop: '4px', textAlign: 'center' }}>
              Connect your wallet to clone this character
            </p>
          )}
        </div>
      )}

      {isOwner && !avatar.cloned_from && (
        <div
          style={{
            marginTop: '12px',
            marginBottom: '8px',
            padding: '10px 12px',
            background: 'var(--panel-raised)',
            borderRadius: '6px',
            border: '1px solid var(--border)',
          }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <strong style={{ fontSize: '13px' }}>Souk Marketplace</strong>
            <span
              className="badge"
              style={{
                background: avatar.listed ? 'var(--good)' : 'var(--border)',
                color: avatar.listed ? '#0d381e' : 'var(--muted)',
              }}
            >
              {avatar.listed ? 'Listed' : 'Unlisted'}
            </span>
          </div>
          <p className="panel-muted" style={{ fontSize: '12px', margin: '4px 0 8px' }}>
            {avatar.listed
              ? 'Other players can clone this character and run your strategy.'
              : 'List this character so others can clone your strategy.'}
          </p>

          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '12px', cursor: 'pointer' }}>
              <input
                type="checkbox"
                checked={avatar.listed}
                disabled={updatingListing}
                onChange={(e) => void handleToggleListing(e.target.checked)}
              />
              <span>Allow cloning</span>
            </label>

            {avatar.listed && (
              <div style={{ display: 'flex', alignItems: 'center', gap: '6px', marginLeft: 'auto' }}>
                <span style={{ fontSize: '12px', color: 'var(--muted)' }}>Price:</span>
                <input
                  type="number"
                  step="0.1"
                  min="0.01"
                  value={editPriceNear}
                  disabled={updatingListing}
                  onChange={(e) => setEditPriceNear(e.target.value)}
                  style={{ width: '70px', padding: '3px 6px', fontSize: '12px' }}
                />
                <span style={{ fontSize: '12px' }}>NEAR</span>
                <button
                  type="button"
                  disabled={updatingListing}
                  onClick={() => void handleSavePrice()}
                  style={{ padding: '3px 8px', fontSize: '12px' }}
                >
                  {updatingListing ? 'Saving…' : 'Save'}
                </button>
              </div>
            )}
          </div>
          {listingError && <p className="panel-error" style={{ margin: '6px 0 0', fontSize: '12px' }}>{listingError}</p>}
        </div>
      )}

      <section>
        <h3>
          Badges{' '}
          <span className="panel-muted">
            {unlockedBadges} / {avatar.badges.length}
          </span>
        </h3>
        <ul className="achievements">
          {avatar.badges.map((badge) => (
            <li
              key={badge.id}
              className={badge.unlocked_at ? 'achievement on' : 'achievement'}
              title={
                badge.unlocked_at
                  ? `${badge.description} Unlocked ${new Date(badge.unlocked_at).toLocaleDateString()}.`
                  : `${badge.description} Locked.`
              }
            >
              <span className="achievement-icon" aria-hidden="true">
                {badgeIcon(badge.id)}
              </span>
              <span className="achievement-name">{badge.name}</span>
              {/* The colour alone must not carry the state (screen readers). */}
              <span className="visually-hidden">{badge.unlocked_at ? ' (unlocked)' : ' (locked)'}</span>
            </li>
          ))}
        </ul>
      </section>

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

      {showCloneModal && (
        <CloneModal
          listing={{
            avatar_id: avatar.id,
            name: avatar.name,
            sprite: avatar.sprite,
            risk: avatar.risk,
            goal: avatar.goal,
            creator: avatar.owner,
            price_yocto: avatar.price_yocto ?? '0',
          }}
          onClose={() => setShowCloneModal(false)}
          onSuccess={(newId) => {
            setShowCloneModal(false)
            onOpenAvatar?.(newId)
          }}
        />
      )}
    </>
  )
}
