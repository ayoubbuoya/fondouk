import { useEffect, useState } from 'react'

import { faceUrl } from '../game/sprites'
import {
  ApiError,
  claimPurchase,
  getListings,
  type MarketplaceListing,
} from '../lib/api'
import { formatNear } from '../lib/format'
import { buyClone, currentAccount } from '../lib/wallet'

interface MarketplacePanelProps {
  /** Opens a character's panel (used for the new clone). */
  onOpenAvatar: (id: string) => void
}

/** What the last action did. `pending` keeps the hash so a failed claim can be retried. */
type Notice =
  | { kind: 'ok'; avatarId: string; already: boolean }
  | { kind: 'error'; text: string; pendingHash?: string }

const GOAL_LABELS: Record<MarketplaceListing['goal'], string> = {
  steady_income: 'steady income',
  max_yield: 'max yield',
  protect_capital: 'protect capital',
}

/**
 * F11: characters for sale, and the Buy button.
 *
 * ```text
 * Buy -> wallet pays (buy_clone) -> tx hash -> POST /marketplace/claim -> clone
 * ```
 *
 * The payment and the clone are two steps: the money moves on chain first, then
 * the backend verifies the transaction and creates the clone. If the second
 * step fails (backend down, tab closed), nothing is lost: the hash can be sent
 * again, here with "Retry" or by pasting it in the box at the bottom.
 * Prices come from the contract; the persona is never shown (D7).
 */
export function MarketplacePanel({ onOpenAvatar }: MarketplacePanelProps) {
  const [listings, setListings] = useState<MarketplaceListing[] | null>(null)
  const [account, setAccount] = useState<string | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<Notice | null>(null)
  const [pastedHash, setPastedHash] = useState('')

  useEffect(() => {
    let cancelled = false
    Promise.all([getListings(), currentAccount()])
      .then(([loaded, who]) => {
        if (cancelled) return
        setListings(loaded)
        setAccount(who)
      })
      .catch((err) => {
        console.error('loading the marketplace failed', err)
        if (!cancelled) setLoadError('Could not load the marketplace.')
      })
    return () => {
      cancelled = true
    }
  }, [])

  /** Sends the hash to the backend; shows the result or an error with a Retry. */
  const claim = async (txHash: string) => {
    try {
      const result = await claimPurchase(txHash)
      setNotice({ kind: 'ok', avatarId: result.avatar_id, already: result.already_claimed })
      setPastedHash('')
    } catch (err) {
      console.error('claim failed', err)
      setNotice({ kind: 'error', text: claimErrorMessage(err), pendingHash: txHash })
    }
  }

  /** Runs one action with the buttons disabled and the old notice cleared. */
  const run = async (action: () => Promise<void>) => {
    setBusy(true)
    setNotice(null)
    try {
      await action()
    } finally {
      setBusy(false)
    }
  }

  const onBuy = (listing: MarketplaceListing) =>
    run(async () => {
      let txHash: string
      try {
        txHash = await buyClone(listing.avatar_id, listing.price_yocto)
      } catch (err) {
        console.error('payment failed', err)
        setNotice({ kind: 'error', text: 'Payment cancelled or failed. Nothing was charged.' })
        return
      }
      await claim(txHash)
    })

  if (loadError) return <p className="panel-error">{loadError}</p>
  if (!listings) return <p className="panel-muted">Loading…</p>

  return (
    <>
      <h2>Marketplace</h2>
      <p className="panel-muted">
        Buy a copy of a character. The strategy stays private: you get a new character that runs
        it, with 10,000 USD of paper money. The creator gets 90% of the price.
      </p>
      {account === null && <p className="panel-muted">Connect your wallet to buy.</p>}

      {notice?.kind === 'ok' && (
        <div className="notice notice-ok">
          <p>{notice.already ? 'This purchase was already claimed.' : 'Your clone is ready.'}</p>
          <button type="button" onClick={() => onOpenAvatar(notice.avatarId)}>
            Open my clone
          </button>
        </div>
      )}
      {notice?.kind === 'error' && (
        <div className="notice notice-error">
          <p>{notice.text}</p>
          {notice.pendingHash && (
            <>
              <p className="mono">{notice.pendingHash}</p>
              <button type="button" disabled={busy} onClick={() => void run(() => claim(notice.pendingHash!))}>
                Retry
              </button>
            </>
          )}
        </div>
      )}

      {listings.length === 0 && <p className="panel-muted">Nothing is for sale yet.</p>}
      <ul className="listings">
        {listings.map((listing) => (
          <li key={listing.avatar_id} className="listing">
            <img src={faceUrl(listing.sprite)} alt="" width={38} height={38} className="panel-face" />
            <div className="listing-text">
              <strong>{listing.name}</strong>
              <span className="panel-muted">
                risk {listing.risk} · goal {GOAL_LABELS[listing.goal]}
              </span>
              <span className="panel-muted">
                by <span className="mono">{listing.creator}</span>
              </span>
            </div>
            <button
              type="button"
              disabled={busy || account === null || account === listing.creator}
              title={account === listing.creator ? 'You cannot buy your own character' : undefined}
              onClick={() => void onBuy(listing)}
            >
              {busy ? '…' : `Buy ${formatNear(listing.price_yocto)}`}
            </button>
          </li>
        ))}
      </ul>

      {account !== null && (
        <section>
          <h3>Paid but no clone?</h3>
          <p className="panel-muted">Paste the transaction hash of your payment.</p>
          <div className="claim-form">
            <input
              value={pastedHash}
              onChange={(event) => setPastedHash(event.target.value)}
              placeholder="transaction hash"
              aria-label="Transaction hash"
            />
            <button
              type="button"
              disabled={busy || pastedHash.trim() === ''}
              onClick={() => void run(() => claim(pastedHash.trim()))}
            >
              Claim
            </button>
          </div>
        </section>
      )}
    </>
  )
}

/** A short text for the user; details stay in the console. */
function claimErrorMessage(err: unknown): string {
  if (err instanceof ApiError && err.status === 401) return 'Log in first, then retry.'
  if (err instanceof ApiError && err.status === 403) return 'This purchase belongs to another account.'
  if (err instanceof ApiError && err.status === 400) return `Payment sent, but: ${err.message}.`
  return 'Payment sent, but the clone is not created yet. Retry in a moment: your money is safe on chain.'
}
