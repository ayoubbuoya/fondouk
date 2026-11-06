import { useEffect, useState } from 'react'

import { ApiError } from '../lib/api'
import { currentAccount, login, logout } from '../lib/wallet'

/** `undefined` = still asking the backend; `null` = nobody logged in. */
type Account = string | null | undefined

/**
 * Top bar: title, data source note (F2 acceptance) and the wallet button.
 * On load it asks `GET /auth/me`, so a page reload keeps the login (F1).
 */
export function Header() {
  const [account, setAccount] = useState<Account>(undefined)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    currentAccount()
      .then(setAccount)
      .catch(() => setAccount(null))
  }, [])

  const run = async (action: () => Promise<void>) => {
    setBusy(true)
    setError(null)
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

  return (
    <header className="header">
      <div className="header-title">
        <h1>Fondouk</h1>
        <span className="header-note">Yield data from DefiLlama, updated hourly</span>
      </div>
      <div className="header-wallet">
        {error && <span className="header-error">{error}</span>}
        {account === undefined ? null : account === null ? (
          <button type="button" onClick={onLogin} disabled={busy}>
            {busy ? 'Waiting for wallet…' : 'Connect wallet'}
          </button>
        ) : (
          <>
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
