import { useEffect, useState } from 'react'

import { ApiError } from '../lib/api'
import { formatApy, formatCompactUsd } from '../lib/format'
import type { TownStats } from '../lib/townStats'
import { claimFaucet, currentAccount, login, logout } from '../lib/wallet'

/** `undefined` = still asking the backend; `null` = nobody logged in. */
type Account = string | null | undefined

interface HudProps {
  /** `null` until the town loaded its villages and characters. */
  stats: TownStats | null
  /** Opens the create-avatar wizard; the button shows only when logged in (D63). */
  onCreateAvatar: () => void
  /** Toggles the simulation demo controls floating bar. */
  onToggleDemo: () => void
  /** Whether the demo bar is currently open. */
  isDemoOpen: boolean
}

/**
 * The bar floating over the top of the town: name, live town numbers, and
 * the wallet (login, faucet, create avatar, logout).
 *
 * ```text
 * FONDOUK  [Agents 10] [Best APY 22.3%] [Pools TVL $3.3M] [Sim $98.4K] [Real $250]   wallet
 * ```
 *
 * Sim and real equity are separate chips with their own badge: the two are
 * never shown as one number (CLAUDE.md "data source labels").
 */
export function Hud({ stats, onCreateAvatar, onToggleDemo, isDemoOpen }: HudProps) {
  const [account, setAccount] = useState<Account>(undefined)
  const [busy, setBusy] = useState(false)
  const [faucetBusy, setFaucetBusy] = useState(false)
  const [faucetMessage, setFaucetMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    currentAccount()
      .then(setAccount)
      .catch(() => setAccount(null))
  }, [])

  const run = async (action: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    setFaucetMessage(null)
    try {
      await action()
    } catch (err) {
      console.error(err)
      setError(loginErrorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  const onLogin = () => run(async () => setAccount(await login()))
  const onLogout = () =>
    run(async () => {
      await logout()
      setAccount(null)
    })

  const onClaimFaucet = async () => {
    setFaucetBusy(true)
    setError(null)
    setFaucetMessage(null)
    try {
      await claimFaucet()
      setFaucetMessage('100 tUSDC claimed')
    } catch (err) {
      console.error(err)
      setError('Faucet claim failed or cancelled.')
    } finally {
      setFaucetBusy(false)
    }
  }

  return (
    <header className="hud">
      <div className="hud-brand">
        <h1>Fondouk</h1>
        <span className="hud-tagline">AI merchants farming real NEAR yields</span>
      </div>

      {stats && (
        <ul className="hud-stats" aria-label="Town numbers">
          <StatChip label="Agents" value={String(stats.agents)} />
          <StatChip label="Best APY" value={formatApy(stats.bestApy)} title="Highest live APY among fresh villages" />
          <StatChip
            label="Pools TVL"
            value={formatCompactUsd(stats.poolsTvlUsd)}
            title="Size of the 4 real pools, from DefiLlama (updated hourly)"
          />
          <StatChip label="Equity" badge="sim" value={formatCompactUsd(stats.simEquityUsd)} title="Paper money of all sim characters" />
          {stats.realAgents > 0 && (
            <StatChip
              label="Equity"
              badge="real"
              value={formatCompactUsd(stats.realEquityUsd)}
              title="Test tokens held in the vault contract"
            />
          )}
        </ul>
      )}

      <div className="hud-wallet">
        <button
          type="button"
          className={`hud-demo-btn${isDemoOpen ? ' active' : ''}`}
          onClick={onToggleDemo}
          title="Toggle Simulation & Demo Controls"
        >
          ⚡ Demo
        </button>
        {faucetMessage && <span className="header-success">{faucetMessage}</span>}
        {error && <span className="header-error">{error}</span>}
        {account === undefined ? null : account === null ? (
          <button type="button" className="primary" onClick={onLogin} disabled={busy}>
            {busy ? 'Waiting for wallet…' : 'Connect wallet'}
          </button>
        ) : (
          <>
            <button
              type="button"
              onClick={onClaimFaucet}
              disabled={faucetBusy || busy}
              title="Claim 100 Test USD (tUSDC) for Real Mode avatars"
            >
              {faucetBusy ? 'Claiming…' : '🚰 Faucet'}
            </button>
            <button type="button" className="primary" onClick={onCreateAvatar}>
              + Create avatar
            </button>
            <span className="header-account" title={account}>
              {account}
            </span>
            <button type="button" onClick={onLogout} disabled={busy}>
              Log out
            </button>
          </>
        )}
      </div>
    </header>
  )
}

interface StatChipProps {
  label: string
  value: string
  /** `sim` or `real`: the data source of this number. */
  badge?: 'sim' | 'real'
  title?: string
}

function StatChip({ label, value, badge, title }: StatChipProps) {
  return (
    <li className="hud-stat" title={title}>
      <span className="hud-stat-label">
        {label}
        {badge && <span className={`badge badge-${badge}`}>{badge}</span>}
      </span>
      <span className="hud-stat-value">{value}</span>
    </li>
  )
}

/** A short text for the user; details stay in the console. */
function loginErrorMessage(err: unknown): string {
  if (err instanceof ApiError && err.status === 401) {
    return 'Login refused: the wallet key must be a full-access key of this account.'
  }
  if (err instanceof ApiError) return `Login failed (${err.status}).`
  return 'Login cancelled or failed.'
}
