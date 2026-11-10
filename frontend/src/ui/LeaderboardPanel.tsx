import { useEffect, useMemo, useState } from 'react'

import { faceUrl } from '../game/sprites'
import { getLeaderboard, type LeaderboardEntry } from '../lib/api'
import { formatAge, formatDrawdown, formatNear, formatPercent } from '../lib/format'

interface LeaderboardPanelProps {
  onSelectAvatar: (avatarId: string) => void
}

type Tab = 'main' | 'new'

type SortKey =
  | 'name'
  | 'owner'
  | 'mode'
  | 'return_30d'
  | 'return_7d'
  | 'max_drawdown'
  | 'moves_count'
  | 'age_days'
  | 'clone_count'
  | 'price'

export function LeaderboardPanel({ onSelectAvatar }: LeaderboardPanelProps) {
  const [entries, setEntries] = useState<LeaderboardEntry[] | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [tab, setTab] = useState<Tab>('main')
  const [sortKey, setSortKey] = useState<SortKey>('return_30d')
  const [sortAsc, setSortAsc] = useState(false)
  const [reloadKey, setReloadKey] = useState(0)

  useEffect(() => {
    let cancelled = false
    getLeaderboard()
      .then((data) => {
        if (cancelled) return
        setEntries(data)
        setError(null)
        // If main has 0 avatars but new has avatars, automatically open new
        const mainCount = data.filter((e) => !e.is_new).length
        const newCount = data.filter((e) => e.is_new).length
        if (mainCount === 0 && newCount > 0) {
          setTab('new')
        }
      })
      .catch((err) => {
        if (cancelled) return
        console.error(err)
        setError('Failed to load leaderboard data.')
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [reloadKey])

  const loadData = () => {
    setLoading(true)
    setReloadKey((k) => k + 1)
  }

  const mainCount = useMemo(() => entries?.filter((e) => !e.is_new).length ?? 0, [entries])
  const newCount = useMemo(() => entries?.filter((e) => e.is_new).length ?? 0, [entries])

  const tabEntries = useMemo(() => {
    if (!entries) return []
    return entries.filter((e) => (tab === 'main' ? !e.is_new : e.is_new))
  }, [entries, tab])

  const sortedEntries = useMemo(() => {
    const list = [...tabEntries]
    list.sort((a, b) => {
      let cmp = 0
      switch (sortKey) {
        case 'name':
          cmp = a.name.localeCompare(b.name)
          break
        case 'owner':
          cmp = a.owner.localeCompare(b.owner)
          break
        case 'mode':
          cmp = a.mode.localeCompare(b.mode)
          break
        case 'return_30d': {
          const aVal = a.return_30d !== null ? parseFloat(a.return_30d) : -999999
          const bVal = b.return_30d !== null ? parseFloat(b.return_30d) : -999999
          cmp = aVal - bVal
          break
        }
        case 'return_7d': {
          const aVal = a.return_7d !== null ? parseFloat(a.return_7d) : -999999
          const bVal = b.return_7d !== null ? parseFloat(b.return_7d) : -999999
          cmp = aVal - bVal
          break
        }
        case 'max_drawdown': {
          const aVal = parseFloat(a.max_drawdown || '0')
          const bVal = parseFloat(b.max_drawdown || '0')
          cmp = aVal - bVal
          break
        }
        case 'moves_count':
          cmp = a.moves_count - b.moves_count
          break
        case 'age_days':
          cmp = a.age_days - b.age_days
          break
        case 'clone_count':
          cmp = a.clone_count - b.clone_count
          break
        case 'price': {
          const aVal = a.listed && a.price_yocto ? BigInt(a.price_yocto) : -1n
          const bVal = b.listed && b.price_yocto ? BigInt(b.price_yocto) : -1n
          cmp = aVal < bVal ? -1 : aVal > bVal ? 1 : 0
          break
        }
      }
      return sortAsc ? cmp : -cmp
    })
    return list
  }, [tabEntries, sortKey, sortAsc])

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortAsc(!sortAsc)
    } else {
      setSortKey(key)
      // Default to descending for returns, moves, clones; ascending for strings
      const defaultDesc = [
        'return_30d',
        'return_7d',
        'max_drawdown',
        'moves_count',
        'age_days',
        'clone_count',
        'price',
      ].includes(key)
      setSortAsc(!defaultDesc)
    }
  }

  const sortIndicator = (key: SortKey) => {
    if (sortKey !== key) return <span className="sort-hint">↕</span>
    return <span className="sort-active">{sortAsc ? '▲' : '▼'}</span>
  }

  return (
    <div className="leaderboard-panel">
      <div className="panel-head">
        <div>
          <h2>Leaderboard</h2>
          <p className="panel-subhead">Live yield-farming ranking of Fondouk agents</p>
        </div>
      </div>

      <div className="leaderboard-tabs" role="tablist" aria-label="Leaderboard tabs">
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'main'}
          className={`leaderboard-tab ${tab === 'main' ? 'active' : ''}`}
          onClick={() => setTab('main')}
        >
          Main (7d+ live) <span className="tab-badge">{mainCount}</span>
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'new'}
          className={`leaderboard-tab ${tab === 'new' ? 'active' : ''}`}
          onClick={() => setTab('new')}
        >
          New (&lt;7d) <span className="tab-badge">{newCount}</span>
        </button>
      </div>

      {loading && <p className="panel-status">Loading leaderboard…</p>}
      {error && (
        <div className="panel-error-box">
          <p className="panel-error">{error}</p>
          <button type="button" onClick={loadData}>
            Retry
          </button>
        </div>
      )}

      {!loading && !error && sortedEntries.length === 0 && (
        <div className="leaderboard-empty">
          <p>No characters in this tab yet.</p>
          {tab === 'main' && newCount > 0 && (
            <button type="button" className="switch-tab-btn" onClick={() => setTab('new')}>
              View New characters ({newCount})
            </button>
          )}
        </div>
      )}

      {!loading && !error && sortedEntries.length > 0 && (
        <div className="leaderboard-table-wrapper">
          <table className="leaderboard-table">
            <thead>
              <tr>
                <th onClick={() => handleSort('name')} className="th-clickable">
                  Avatar {sortIndicator('name')}
                </th>
                <th onClick={() => handleSort('owner')} className="th-clickable">
                  Owner {sortIndicator('owner')}
                </th>
                <th onClick={() => handleSort('mode')} className="th-clickable">
                  Mode {sortIndicator('mode')}
                </th>
                <th onClick={() => handleSort('return_30d')} className="th-clickable th-num">
                  30d Return {sortIndicator('return_30d')}
                </th>
                <th onClick={() => handleSort('return_7d')} className="th-clickable th-num">
                  7d Return {sortIndicator('return_7d')}
                </th>
                <th onClick={() => handleSort('max_drawdown')} className="th-clickable th-num">
                  Max Drawdown {sortIndicator('max_drawdown')}
                </th>
                <th onClick={() => handleSort('moves_count')} className="th-clickable th-num">
                  Moves {sortIndicator('moves_count')}
                </th>
                <th onClick={() => handleSort('age_days')} className="th-clickable th-num">
                  Age {sortIndicator('age_days')}
                </th>
                <th onClick={() => handleSort('clone_count')} className="th-clickable th-num">
                  Clones {sortIndicator('clone_count')}
                </th>
                <th onClick={() => handleSort('price')} className="th-clickable th-num">
                  Price {sortIndicator('price')}
                </th>
              </tr>
            </thead>
            <tbody>
              {sortedEntries.map((row) => {
                const ret30 = row.return_30d !== null ? parseFloat(row.return_30d) : null
                const ret7 = row.return_7d !== null ? parseFloat(row.return_7d) : null
                const dd = parseFloat(row.max_drawdown || '0')

                return (
                  <tr
                    key={row.id}
                    className="leaderboard-row"
                    onClick={() => onSelectAvatar(row.id)}
                    title="Click to view avatar details"
                  >
                    <td className="td-avatar">
                      <img src={faceUrl(row.sprite)} alt="" className="leaderboard-face" />
                      <span className="leaderboard-name">{row.name}</span>
                    </td>
                    <td className="td-owner" title={row.owner}>
                      {truncateAccount(row.owner)}
                    </td>
                    <td>
                      <span className={`badge-${row.mode}`}>{row.mode}</span>
                    </td>
                    <td
                      className={`th-num ${
                        ret30 === null
                          ? 'val-muted'
                          : ret30 > 0
                            ? 'val-pos'
                            : ret30 < 0
                              ? 'val-neg'
                              : 'val-muted'
                      }`}
                    >
                      {formatPercent(row.return_30d)}
                    </td>
                    <td
                      className={`th-num ${
                        ret7 === null
                          ? 'val-muted'
                          : ret7 > 0
                            ? 'val-pos'
                            : ret7 < 0
                              ? 'val-neg'
                              : 'val-muted'
                      }`}
                    >
                      {formatPercent(row.return_7d)}
                    </td>
                    <td className={`th-num ${dd > 0 ? 'val-neg' : 'val-muted'}`}>
                      {formatDrawdown(row.max_drawdown)}
                    </td>
                    <td className="th-num">{row.moves_count}</td>
                    <td className="th-num">{formatAge(row.age_days)}</td>
                    <td className="th-num">{row.clone_count}</td>
                    <td className="th-num">
                      {row.listed && row.price_yocto ? (
                        <span className="price-tag">{formatNear(row.price_yocto)}</span>
                      ) : (
                        <span className="val-muted">—</span>
                      )}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

function truncateAccount(account: string): string {
  if (account.length <= 16) return account
  return `${account.slice(0, 8)}…${account.slice(-5)}`
}
