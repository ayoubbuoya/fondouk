import { useState } from 'react'

import { faceUrl, SPRITES, type SpriteId } from '../game/sprites'
import {
  claimPurchase,
  cloneFree,
  type AvatarMode,
  type MarketplaceListing,
} from '../lib/api'
import { formatNear } from '../lib/format'
import { buyClone } from '../lib/wallet'

interface CloneModalProps {
  listing: MarketplaceListing
  onClose: () => void
  onSuccess: (newAvatarId: string) => void
}

const SIM_CAPITALS = [1_000, 10_000, 100_000] as const
const REAL_CAPITALS = [10, 25, 50, 100] as const

export function CloneModal({ listing, onClose, onSuccess }: CloneModalProps) {
  const [name, setName] = useState(`${listing.name} II`)
  const [sprite, setSprite] = useState<SpriteId>(
    (SPRITES as readonly string[]).includes(listing.sprite)
      ? (listing.sprite as SpriteId)
      : 'boy',
  )
  const [mode, setMode] = useState<AvatarMode>('sim')
  const [capital, setCapital] = useState<number>(10_000)
  const [busy, setBusy] = useState(false)
  const [statusText, setStatusText] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const isFree = !listing.price_yocto || listing.price_yocto === '0'
  const trimmedName = name.trim()
  const isValid = trimmedName.length > 0 && [...trimmedName].length <= 32

  const handleConfirm = async () => {
    if (!isValid || busy) return
    setBusy(true)
    setError(null)

    try {
      if (isFree) {
        setStatusText('Creating clone agent…')
        const res = await cloneFree({
          avatar_id: listing.avatar_id,
          name: trimmedName,
          sprite,
          starting_capital_usd: capital,
          mode,
        })
        onSuccess(res.avatar_id)
      } else {
        setStatusText('Confirm payment in your wallet…')
        let txHash: string
        try {
          txHash = await buyClone(listing.avatar_id, listing.price_yocto, listing.creator)
        } catch (err) {
          console.error('Wallet payment failed or was cancelled', err)
          setError('Payment was cancelled or rejected by wallet.')
          setBusy(false)
          setStatusText(null)
          return
        }

        setStatusText('Verifying on-chain transaction & building agent…')
        const res = await claimPurchase({
          tx_hash: txHash,
          name: trimmedName,
          sprite,
          starting_capital_usd: capital,
          mode,
        })
        onSuccess(res.avatar_id)
      }
    } catch (err) {
      console.error('Clone creation failed', err)
      setError(err instanceof Error ? err.message : 'Could not create clone. Please try again.')
      setBusy(false)
      setStatusText(null)
    }
  }

  return (
    <div className="clone-modal-backdrop" onClick={onClose}>
      <div className="clone-modal-card" onClick={(e) => e.stopPropagation()}>
        <button
          type="button"
          className="clone-modal-close"
          onClick={onClose}
          aria-label="Close modal"
        >
          ✕
        </button>

        <div className="clone-modal-header">
          <img
            src={faceUrl(listing.sprite)}
            alt=""
            width={48}
            height={48}
            className="panel-face"
          />
          <div>
            <h3>Clone Strategy: {listing.name}</h3>
            <p className="panel-muted">
              by <span className="mono">{listing.creator}</span> · {listing.risk} risk · {listing.goal.replace('_', ' ')}
            </p>
          </div>
        </div>

        <div className="clone-form-section">
          <label htmlFor="clone-name">
            <strong>Clone Character Name</strong> ({[...name.trim()].length}/32)
          </label>
          <input
            id="clone-name"
            type="text"
            value={name}
            maxLength={32}
            disabled={busy}
            onChange={(e) => setName(e.target.value)}
            placeholder="Name your agent"
          />
        </div>

        <div className="clone-form-section">
          <label>
            <strong>Choose Character Face</strong>
          </label>
          <div className="clone-sprite-grid">
            {SPRITES.map((s) => (
              <button
                key={s}
                type="button"
                className={`clone-sprite-btn ${sprite === s ? 'active' : ''}`}
                disabled={busy}
                onClick={() => setSprite(s)}
                title={s}
              >
                <img src={faceUrl(s)} alt={s} width={36} height={36} />
              </button>
            ))}
          </div>
        </div>

        <div className="clone-form-section">
          <label>
            <strong>Execution Mode</strong>
          </label>
          <div className="clone-pill-group">
            <button
              type="button"
              className={`clone-pill-btn ${mode === 'sim' ? 'active' : ''}`}
              disabled={busy}
              onClick={() => {
                setMode('sim')
                setCapital(10_000)
              }}
            >
              🎮 Sim (Paper Ledger)
            </button>
            <button
              type="button"
              className={`clone-pill-btn ${mode === 'real' ? 'active' : ''}`}
              disabled={busy}
              onClick={() => {
                setMode('real')
                setCapital(25)
              }}
            >
              ⚡ Real (Vault tUSDC)
            </button>
          </div>
        </div>

        <div className="clone-form-section">
          <label>
            <strong>Starting Capital ({mode === 'sim' ? 'USD Paper' : 'tUSDC Vault'})</strong>
          </label>
          <div className="clone-pill-group">
            {(mode === 'sim' ? SIM_CAPITALS : REAL_CAPITALS).map((amt) => (
              <button
                key={amt}
                type="button"
                className={`clone-pill-btn ${capital === amt ? 'active' : ''}`}
                disabled={busy}
                onClick={() => setCapital(amt)}
              >
                {mode === 'sim' ? `$${amt.toLocaleString()}` : `${amt} tUSDC`}
              </button>
            ))}
          </div>
        </div>

        <div className="clone-price-box">
          <div className="clone-price-row">
            <span>Clone License Fee:</span>
            <strong>{isFree ? 'Free' : `${formatNear(listing.price_yocto)} NEAR`}</strong>
          </div>
          <p className="panel-muted" style={{ fontSize: '11px', margin: '4px 0 0' }}>
            {isFree
              ? 'Free license. The secret trading strategy runs on your clone automatically.'
              : '90% goes to creator, 10% platform fee. The secret strategy is linked on-chain.'}
          </p>
        </div>

        {error && <p className="panel-error">{error}</p>}
        {statusText && <p className="panel-muted mono" style={{ color: 'var(--accent)' }}>{statusText}</p>}

        <div className="clone-modal-actions">
          <button
            type="button"
            className="btn-secondary"
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn-primary"
            disabled={busy || !isValid}
            onClick={() => void handleConfirm()}
          >
            {busy ? 'Processing…' : isFree ? 'Clone for Free' : `Pay & Clone (${formatNear(listing.price_yocto)} NEAR)`}
          </button>
        </div>
      </div>
    </div>
  )
}
