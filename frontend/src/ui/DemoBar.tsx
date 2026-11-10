import { useEffect, useRef, useState } from 'react'

import {
  resetRealRates,
  triggerDemoStep,
  triggerMarketShock,
  triggerTick,
} from '../lib/demoApi'

interface DemoBarProps {
  onClose: () => void
}

type AutoPace = 'off' | '5s' | '10s'

export function DemoBar({ onClose }: DemoBarProps) {
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState<string>('Ready for demo actions.')
  const [autoPace, setAutoPace] = useState<AutoPace>('off')
  const cycleRef = useRef(0)

  // Auto-pace simulation loop
  useEffect(() => {
    if (autoPace === 'off') return

    const intervalMs = autoPace === '5s' ? 5000 : 10000
    const presets: Array<'usdt_spike' | 'dex_spike' | 'usdc_drop'> = [
      'usdt_spike',
      'dex_spike',
      'usdc_drop',
    ]

    const timer = setInterval(async () => {
      const preset = presets[cycleRef.current % presets.length]
      cycleRef.current += 1
      try {
        setStatus(`Auto: stepping with ${preset}…`)
        const res = await triggerDemoStep(preset)
        setStatus(`Auto: ${res.shock.village_id} at ${res.shock.apy}% (${res.tick.decisions} decided)`)
      } catch (err) {
        console.warn('auto-step failed', err)
      }
    }, intervalMs)

    return () => clearInterval(timer)
  }, [autoPace])

  const handleShock = async (preset: 'usdt_spike' | 'dex_spike' | 'usdc_drop') => {
    setBusy(true)
    try {
      const res = await triggerMarketShock(preset)
      setStatus(`Market shock: ${res.village_id} APY jumped to ${res.apy}%!`)
    } catch (err) {
      console.error(err)
      setStatus('Failed to trigger market shock.')
    } finally {
      setBusy(false)
    }
  }

  const handleTick = async () => {
    setBusy(true)
    setStatus('Running agent tick across active characters…')
    try {
      const res = await triggerTick(true, true)
      setStatus(`Agent tick completed! ${res.decisions} decisions executed.`)
    } catch (err) {
      console.error(err)
      setStatus('Tick failed. (Is LLM configured?)')
    } finally {
      setBusy(false)
    }
  }

  const handleStep = async (preset: 'usdt_spike' | 'dex_spike') => {
    setBusy(true)
    setStatus(`Executing one-click shock & tick (${preset})…`)
    try {
      const res = await triggerDemoStep(preset)
      setStatus(`One-click step done! ${res.shock.village_id} at ${res.shock.apy}% (${res.tick.decisions} decided)`)
    } catch (err) {
      console.error(err)
      setStatus('Step failed.')
    } finally {
      setBusy(false)
    }
  }

  const handleReset = async () => {
    setBusy(true)
    try {
      await resetRealRates()
      setStatus('Market APYs reset to normal DefiLlama baselines.')
    } catch (err) {
      console.error(err)
      setStatus('Reset failed.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <aside className="demo-bar" aria-label="Demo Controller">
      <div className="demo-bar-header">
        <div className="demo-bar-title">
          <span className="demo-pill">⚡ DEMO</span>
          <strong>Simulation Remote Control</strong>
        </div>
        <button
          type="button"
          className="demo-close-btn"
          onClick={onClose}
          aria-label="Close demo controller"
          title="Close demo controller"
        >
          ✕
        </button>
      </div>

      <div className="demo-bar-sections">
        {/* Yield Shocks */}
        <div className="demo-group">
          <span className="demo-label">Market Yields:</span>
          <div className="demo-actions">
            <button
              type="button"
              disabled={busy}
              onClick={() => handleShock('usdt_spike')}
              title="Surge Rhea Lend USDT to 22.5%"
            >
              🔥 USDT 22.5%
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => handleShock('dex_spike')}
              title="Surge Rhea FRAX-USDC DEX pool to 25.0%"
            >
              🔥 DEX 25%
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => handleShock('usdc_drop')}
              title="Dip Rhea Lend USDC to 3.2%"
            >
              📉 USDC 3.2%
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={handleReset}
              title="Reset APYs to real baseline"
            >
              🔄 Reset
            </button>
          </div>
        </div>

        {/* Ticks */}
        <div className="demo-group">
          <span className="demo-label">Agent Decisions:</span>
          <div className="demo-actions">
            <button
              type="button"
              className="primary"
              disabled={busy}
              onClick={handleTick}
              title="Run tick right now for all active characters"
            >
              {busy ? 'Deciding…' : '▶️ Run Tick'}
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => handleStep('usdt_spike')}
              title="Surge USDT and immediately run tick"
            >
              ⚡ Shock & Step
            </button>
          </div>
        </div>

        {/* Auto pacing */}
        <div className="demo-group">
          <span className="demo-label">Auto Pace:</span>
          <div className="demo-pacing-buttons">
            <button
              type="button"
              className={autoPace === 'off' ? 'active' : ''}
              onClick={() => setAutoPace('off')}
            >
              Off
            </button>
            <button
              type="button"
              className={autoPace === '5s' ? 'active' : ''}
              onClick={() => setAutoPace('5s')}
              title="Auto step every 5 seconds"
            >
              5s
            </button>
            <button
              type="button"
              className={autoPace === '10s' ? 'active' : ''}
              onClick={() => setAutoPace('10s')}
              title="Auto step every 10 seconds"
            >
              10s
            </button>
          </div>
        </div>
      </div>

      <div className="demo-bar-status">
        <span className="demo-status-bullet">●</span>
        <span className="demo-status-text">{status}</span>
      </div>
    </aside>
  )
}
