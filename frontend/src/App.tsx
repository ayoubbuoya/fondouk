import { useEffect, useMemo, useState } from 'react'

import { townBus } from './game/townBus'
import { getArenaFeed, type AvatarSummary, type Village } from './lib/api'
import type { LiveEvent } from './lib/liveEvents'
import { arenaLine, pushLine, tickerLine, type TickerLine, type TickerNames } from './lib/ticker'
import { townStats } from './lib/townStats'
import { ArenaPanel } from './ui/ArenaPanel'
import { AvatarPanel } from './ui/AvatarPanel'
import { CreateAvatarPanel } from './ui/CreateAvatarPanel'
import { DemoBar } from './ui/DemoBar'
import { Hud } from './ui/Hud'
import { IntroCard } from './ui/IntroCard'
import { JudgeTour } from './ui/JudgeTour'
import { getTourStep } from './ui/judgeTourModel'
import { LeaderboardPanel } from './ui/LeaderboardPanel'
import { MarketplacePanel } from './ui/MarketplacePanel'
import { NavDock } from './ui/NavDock'
import { SidePanel } from './ui/SidePanel'
import { Ticker } from './ui/Ticker'
import { TownCanvas, type Selection } from './ui/TownCanvas'
import { VillagePanel } from './ui/VillagePanel'

/** The floating panel on the right: one at a time. */
type Panel = 'none' | 'marketplace' | 'create' | 'leaderboard' | 'arena'

/** How many past arena messages fill the news strip at page load. */
const TICKER_SEED_MESSAGES = 10

/**
 * Layout: the town fills the window and everything else floats on top of it,
 * like a game HUD.
 *
 * ```text
 * ┌ Hud: name · town numbers · wallet ───────────────────┐
 * │ NavDock                                   SidePanel  │   the canvas never
 * │ (left)            TownCanvas              (right)    │   resizes when a
 * │                   (full window)                      │   panel opens
 * └ Ticker: town news ───────────────────────────────────┘
 * ```
 *
 * The town loads the villages and characters once and reports them here
 * (`onVillages`, `onAvatars`), so the Hud and the Ticker need no extra request.
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
  const [tourStep, setTourStep] = useState<number | null>(null)
  const [villages, setVillages] = useState<Village[] | null>(null)
  const [avatars, setAvatars] = useState<AvatarSummary[] | null>(null)
  const [news, setNews] = useState<TickerLine[]>([])
  const [demoOpen, setDemoOpen] = useState(false)

  const stats = useMemo(
    () => (villages && avatars ? townStats(villages, avatars) : null),
    [villages, avatars],
  )
  const names = useMemo<TickerNames>(
    () => ({
      avatars: new Map((avatars ?? []).map((avatar) => [avatar.id, avatar.name])),
      villages: new Map((villages ?? []).map((village) => [village.id, village.name])),
    }),
    [villages, avatars],
  )

  // The news strip starts with the latest real arena messages (each shows its
  // age), so it is not empty between two agent ticks.
  useEffect(() => {
    let cancelled = false
    getArenaFeed(TICKER_SEED_MESSAGES)
      .then((messages) => {
        if (cancelled) return
        // Oldest first, so the newest message ends up in front.
        const oldestFirst = [...messages].sort((a, b) => a.ts.localeCompare(b.ts))
        setNews((lines) => oldestFirst.reduce((all, message) => pushLine(all, arenaLine(message)), lines))
      })
      .catch((err) => console.warn('loading the arena feed for the news strip failed', err))
    return () => {
      cancelled = true
    }
  }, [])

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

  // Judge tour (F12.2): actively drives the UI to highlight each feature.
  const goToTourStep = (stepIdx: number) => {
    setTourStep(stepIdx)
    const step = getTourStep(stepIdx)
    if (step.cameraFocus) {
      townBus.emit('camera:focus', step.cameraFocus)
    }
    if (step.target.selectAvatar) {
      setPanel('none')
      setSelection({ kind: 'avatar', id: 'a0000000-0000-4000-8000-000000000001' })
    } else if (step.target.panel !== 'none') {
      setSelection(null)
      setPanel(step.target.panel)
    } else {
      setSelection(null)
      setPanel('none')
    }
  }

  const startTour = () => {
    goToTourStep(0)
  }

  const closeTour = () => {
    setTourStep(null)
    townBus.emit('camera:focus', { reset: true })
  }

  // A new character sends no live event while paused, so the town is told to
  // reload its list; then its panel opens.
  const onCreated = (avatarId: string) => {
    townBus.emit('avatars:reload')
    select({ kind: 'avatar', id: avatarId })
  }

  const onLiveEvent = (event: LiveEvent) => {
    const line = tickerLine(event, names)
    if (line) setNews((lines) => pushLine(lines, line))

    if (!selection) return
    const concernsPanel =
      selection.kind === 'avatar'
        ? event.type === 'decision' && event.avatar_id === selection.id
        : event.type === 'apr_update' || event.type === 'decision'
    if (concernsPanel) setRefreshKey((key) => key + 1)
  }

  const activeDockItem =
    tourStep !== null
      ? 'tour'
      : panel === 'arena' || panel === 'leaderboard' || panel === 'marketplace'
        ? panel
        : null

  return (
    <div className="app">
      <TownCanvas
        onSelect={select}
        onOpenArena={() => open('arena')}
        onOpenLandmark={open}
        onVillages={setVillages}
        onAvatars={setAvatars}
        onLiveEvent={onLiveEvent}
      />

      <Hud
        stats={stats}
        onCreateAvatar={() => open('create')}
        onToggleDemo={() => setDemoOpen((prev) => !prev)}
        isDemoOpen={demoOpen}
      />
      {demoOpen && <DemoBar onClose={() => setDemoOpen(false)} />}
      <NavDock
        active={activeDockItem}
        onStartTour={startTour}
        onOpenArena={() => open('arena')}
        onOpenLeaderboard={() => open('leaderboard')}
        onOpenMarketplace={() => open('marketplace')}
      />
      <Ticker lines={news} onSelectAvatar={(id) => select({ kind: 'avatar', id })} />

      {tourStep !== null && <JudgeTour stepIndex={tourStep} onStepChange={goToTourStep} onClose={closeTour} />}
      {panel === 'arena' && (
        <SidePanel onClose={() => setPanel('none')}>
          <ArenaPanel onSelectAvatar={(id) => select({ kind: 'avatar', id })} />
        </SidePanel>
      )}
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
            <AvatarPanel
              avatarId={selection.id}
              refreshKey={refreshKey}
              onOpenAvatar={(id) => select({ kind: 'avatar', id })}
            />
          ) : (
            <VillagePanel
              villageId={selection.id}
              refreshKey={refreshKey}
              onSelectAvatar={(id) => setSelection({ kind: 'avatar', id })}
            />
          )}
        </SidePanel>
      )}

      <IntroCard onStartTour={startTour} />
    </div>
  )
}
