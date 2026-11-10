import { useEffect, useState } from 'react'

import { ApiError } from '../lib/api'
import { claimFaucet, currentAccount, login, logout } from '../lib/wallet'

/** `undefined` = still asking the backend; `null` = nobody logged in. */
type Account = string | null | undefined

interface HeaderProps {
  onOpenLeaderboard: () => void
  onOpenMarketplace: () => void
  /** Opens the create-avatar wizard; the button shows only when logged in. */
  onCreateAvatar: () => void
}

/**
 * Top bar: title, data source note (F2 acceptance), Leaderboard, Marketplace,
 * Create avatar button (logged in only, D63) and wallet button.
 */
export function Header({ onOpenLeaderboard, onOpenMarketplace, onCreateAvatar }: HeaderProps) {
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
  const onLogout = () => run(async () => {
    await logout()
    setAccount(null)
  })

  const onClaimFaucet = async () => {
    setFaucetBusy(true)
    setError(null)
    setFaucetMessage(null)
    try {
      await claimFaucet()
      setFaucetMessage('100 tUSDC claimed!')
    } catch (err) {
      console.error(err)
      setError('Faucet claim failed or cancelled.')
    } finally {
      setFaucetBusy(false)
    }
  }

  return (
    <header className="header">
      <div className="header-title">
        <h1>Fondouk</h1>
        <span className="header-note">Yield data from DefiLlama, updated hourly</span>
      </div>
      <div className="header-wallet">
        <button type="button" onClick={onOpenLeaderboard}>
          Leaderboard
        </button>
        <button type="button" onClick={onOpenMarketplace}>
          Marketplace
        </button>
        {faucetMessage && <span className="header-success" style={{ color: '#4ade80', fontSize: '0.85rem' }}>{faucetMessage}</span>}
        {error && <span className="header-error">{error}</span>}
        {account === undefined ? null : account === null ? (
          <button type="button" onClick={onLogin} disabled={busy}>
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
              {faucetBusy ? 'Claiming…' : '🚰 Faucet (100 tUSDC)'}
            </button>
            <button type="button" onClick={onCreateAvatar}>
              Create avatar
            </button>
            <span className="header-account">{account}</span>
            <button type="button" onClick={onLogout} disabled={busy}>
              Log out
            </button>
          </>
        )}
      </div>
    </header>
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
