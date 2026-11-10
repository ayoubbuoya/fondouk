import { useState } from 'react'

import { townBus } from './game/townBus'
import type { LiveEvent } from './lib/liveEvents'
import { AvatarPanel } from './ui/AvatarPanel'
import { CreateAvatarPanel } from './ui/CreateAvatarPanel'
import { Header } from './ui/Header'
import { LeaderboardPanel } from './ui/LeaderboardPanel'
import { MarketplacePanel } from './ui/MarketplacePanel'
import { SidePanel } from './ui/SidePanel'
import { TownCanvas, type Selection } from './ui/TownCanvas'
import { VillagePanel } from './ui/VillagePanel'

/** The panel on the right: one at a time. */
type Panel = 'none' | 'marketplace' | 'create' | 'leaderboard'

/**
 * Layout: header on top, town on the left, details panel on the right when
 * something is selected (F8.3, F8.4), or the marketplace (F11), or the
 * create-avatar wizard (F3, D63).
 *
 * `refreshKey` goes up when a live event concerns what the panel shows, so
 * the panel reloads without a page refresh (F8.5):
 * - avatar panel: a `decision` for that character;
 * - village panel: any `apr_update` or `decision` (residents may change).
 */
export default function App() {
  const [selection, setSelection] = useState<Selection | null>(null)
  const [panel, setPanel] = useState<Panel>('none')
  const [refreshKey, setRefreshKey] = useState(0)

  // One panel at a time: picking something on the map closes the marketplace
  // or the wizard, and opening one of those clears the map selection.
  const select = (next: Selection | null) => {
    setSelection(next)
    setPanel('none')
  }
  const open = (next: Panel) => {
    setSelection(null)
    setPanel(next)
  }

  // A new character sends no live event while paused, so the town is told to
  // reload its list; then its panel opens.
  const onCreated = (avatarId: string) => {
    townBus.emit('avatars:reload')
    select({ kind: 'avatar', id: avatarId })
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
      <Header
        onOpenLeaderboard={() => open('leaderboard')}
        onOpenMarketplace={() => open('marketplace')}
        onCreateAvatar={() => open('create')}
      />
      <main className="app-main">
        <TownCanvas onSelect={select} onLiveEvent={onLiveEvent} />
        {panel === 'leaderboard' && (
          <SidePanel onClose={() => setPanel('none')} className="side-panel-wide">
            <LeaderboardPanel onSelectAvatar={(id) => select({ kind: 'avatar', id })} />
          </SidePanel>
        )}
        {panel === 'marketplace' && (
          <SidePanel onClose={() => setPanel('none')}>
            <MarketplacePanel onOpenAvatar={(id) => select({ kind: 'avatar', id })} />
          </SidePanel>
        )}
        {panel === 'create' && (
          <SidePanel onClose={() => setPanel('none')}>
            <CreateAvatarPanel onDone={onCreated} />
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
