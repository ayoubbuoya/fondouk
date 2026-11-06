import { useState } from 'react'

import type { LiveEvent } from './lib/liveEvents'
import { AvatarPanel } from './ui/AvatarPanel'
import { Header } from './ui/Header'
import { MarketplacePanel } from './ui/MarketplacePanel'
import { SidePanel } from './ui/SidePanel'
import { TownCanvas, type Selection } from './ui/TownCanvas'
import { VillagePanel } from './ui/VillagePanel'

/**
 * Layout: header on top, town on the left, details panel on the right when
 * something is selected (F8.3, F8.4).
 *
 * `refreshKey` goes up when a live event concerns what the panel shows, so
 * the panel reloads without a page refresh (F8.5):
 * - avatar panel: a `decision` for that character;
 * - village panel: any `apr_update` or `decision` (residents may change).
 */
export default function App() {
  const [selection, setSelection] = useState<Selection | null>(null)
  const [marketplaceOpen, setMarketplaceOpen] = useState(false)
  const [refreshKey, setRefreshKey] = useState(0)

  // One panel at a time: picking something on the map closes the marketplace,
  // and opening the marketplace clears the map selection.
  const select = (next: Selection | null) => {
    setSelection(next)
    setMarketplaceOpen(false)
  }
  const openMarketplace = () => {
    setSelection(null)
    setMarketplaceOpen(true)
  }

  const onLiveEvent = (event: LiveEvent) => {
    if (!selection) return
    const concernsPanel =
      selection.kind === 'avatar'
        ? event.type === 'decision' && event.avatar_id === selection.id
        : event.type === 'apr_update' || event.type === 'decision'
    if (concernsPanel) setRefreshKey((key) => key + 1)
  }

  return (
    <div className="app">
      <Header onOpenMarketplace={openMarketplace} />
      <main className="app-main">
        <TownCanvas onSelect={select} onLiveEvent={onLiveEvent} />
        {marketplaceOpen && (
          <SidePanel onClose={() => setMarketplaceOpen(false)}>
            <MarketplacePanel onOpenAvatar={(id) => select({ kind: 'avatar', id })} />
          </SidePanel>
        )}
        {selection && (
          <SidePanel onClose={() => setSelection(null)}>
            {selection.kind === 'avatar' ? (
              <AvatarPanel avatarId={selection.id} refreshKey={refreshKey} />
            ) : (
              <VillagePanel
                villageId={selection.id}
                refreshKey={refreshKey}
                onSelectAvatar={(id) => setSelection({ kind: 'avatar', id })}
              />
            )}
          </SidePanel>
        )}
      </main>
    </div>
  )
}
