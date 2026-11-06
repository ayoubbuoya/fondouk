import { useEffect, useState } from 'react'

import { faceUrl } from '../game/sprites'
import { getAvatars, getVillageHistory, getVillages, type AvatarSummary, type Village, type YieldPoint } from '../lib/api'
import { formatApy, formatUsd, staleLabel, totalApy } from '../lib/format'
import { LineChart } from './LineChart'

/** The longest window `GET /villages/{id}/history` allows. */
const HISTORY_DAYS = 30

interface VillagePanelProps {
  villageId: string
  /** Changes on live `apr_update` / `decision` events: reload. */
  refreshKey: number
  /** A character in the list was clicked. */
  onSelectAvatar: (id: string) => void
}

interface VillageData {
  village: Village
  history: YieldPoint[]
  residents: AvatarSummary[]
  /** Computed at load time: rendering must not read the clock. */
  stale: string | null
}

/**
 * F8.4: APY history, TVL, and the characters inside. "Inside" = the village
 * holds most of their money (`main_village`), which is also where their
 * sprite stands.
 */
export function VillagePanel({ villageId, refreshKey, onSelectAvatar }: VillagePanelProps) {
  const [data, setData] = useState<VillageData | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      const [villages, history, avatars] = await Promise.all([
        getVillages(),
        getVillageHistory(villageId, HISTORY_DAYS),
        getAvatars(),
      ])
      const village = villages.find((candidate) => candidate.id === villageId)
      if (!village) throw new Error(`unknown village ${villageId}`)
      const residents = avatars.filter((avatar) => avatar.main_village === villageId)
      if (!cancelled) {
        setData({ village, history, residents, stale: staleLabel(village, new Date()) })
        setError(null)
      }
    }
    load().catch((err) => {
      console.error('loading the village failed', err)
      if (!cancelled) setError('Could not load this village.')
    })
    return () => {
      cancelled = true
    }
  }, [villageId, refreshKey])

  if (error) return <p className="panel-error">{error}</p>
  if (!data || data.village.id !== villageId) return <p className="panel-muted">Loading…</p>

  const { village, history, residents, stale } = data

  return (
    <>
      <div className="panel-head">
        <div>
          <h2>{village.name}</h2>
          <p className="panel-muted">
            {village.kind === 'lending' ? 'lending' : 'liquidity pool'} · {village.asset}
          </p>
        </div>
      </div>
      {stale && <p className="panel-error">{stale}: agents cannot move money in.</p>}

      <section>
        <h3>APY, last {HISTORY_DAYS} days</h3>
        <p className="panel-big">{formatApy(totalApy(village))}</p>
        <LineChart
          values={history.map((point) => totalApy(point) ?? 0)}
          format={formatApy}
          emptyText="Not enough history yet."
          label={`APY of ${village.name}, last ${HISTORY_DAYS} days`}
        />
        <p className="panel-muted">
          TVL {formatUsd(village.tvl_usd)} · source DefiLlama
        </p>
      </section>

      <section>
        <h3>Characters inside ({residents.length})</h3>
        {residents.length === 0 && <p className="panel-muted">Nobody holds most of their money here.</p>}
        <ul className="panel-list">
          {residents.map((avatar) => (
            <li key={avatar.id}>
              <button className="link" onClick={() => onSelectAvatar(avatar.id)}>
                <img src={faceUrl(avatar.sprite)} alt="" width={20} height={20} />
                {avatar.name}
              </button>
              <span>
                {formatUsd(avatar.equity_usd)} <span className={`badge badge-${avatar.mode}`}>{avatar.mode}</span>
              </span>
            </li>
          ))}
        </ul>
      </section>
    </>
  )
}
