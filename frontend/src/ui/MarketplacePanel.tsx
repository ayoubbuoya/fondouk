import { useEffect, useState } from 'react'

import { faceUrl } from '../game/sprites'
import {
  claimPurchase,
  getListings,
  type MarketplaceListing,
} from '../lib/api'
import { formatNear } from '../lib/format'
import { currentAccount } from '../lib/wallet'
import { CloneModal } from './CloneModal'

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

export function MarketplacePanel({ onOpenAvatar }: MarketplacePanelProps) {
  const [listings, setListings] = useState<MarketplaceListing[] | null>(null)
  const [account, setAccount] = useState<string | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<Notice | null>(null)
  const [pastedHash, setPastedHash] = useState('')
  const [selectedForClone, setSelectedForClone] = useState<MarketplaceListing | null>(null)

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
      setNotice({ kind: 'error', text: 'Could not claim transaction.', pendingHash: txHash })
    }
  }

  const run = async (action: () => Promise<void>) => {
    setBusy(true)
    setNotice(null)
    try {
      await action()
    } finally {
      setBusy(false)
    }
  }

  if (loadError) return <p className="panel-error">{loadError}</p>
  if (!listings) return <p className="panel-muted">Loading…</p>

  return (
    <>
      <h2>Souk (Marketplace)</h2>
      <p className="panel-muted">
        Clone an autonomous trading agent. The private strategy stays secret: you get your own
        customized character running it. The creator receives 90% of the clone license fee.
      </p>
      {account === null && <p className="panel-muted">Connect your wallet to clone characters.</p>}

      {notice?.kind === 'ok' && (
        <div className="notice notice-ok">
          <p>{notice.already ? 'This purchase was already claimed.' : 'Your clone is ready!'}</p>
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
              title={account === listing.creator ? 'You cannot clone your own character' : undefined}
              onClick={() => setSelectedForClone(listing)}
            >
              {listing.price_yocto === '0' || !listing.price_yocto
                ? 'Clone (Free)'
                : `Clone (${formatNear(listing.price_yocto)})`}
            </button>
          </li>
        ))}
      </ul>

      {selectedForClone && (
        <CloneModal
          listing={selectedForClone}
          onClose={() => setSelectedForClone(null)}
          onSuccess={(newId) => {
            setSelectedForClone(null)
            onOpenAvatar(newId)
          }}
        />
      )}

      {account !== null && (
        <details style={{ marginTop: '24px', opacity: 0.8 }}>
          <summary style={{ cursor: 'pointer', fontSize: '12px', color: 'var(--muted)' }}>
            Paid but no clone? Paste transaction hash
          </summary>
          <div className="claim-form" style={{ marginTop: '8px' }}>
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
        </details>
      )}
    </>
  )
}
