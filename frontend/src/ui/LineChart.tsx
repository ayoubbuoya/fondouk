/**
 * A small line chart drawn with plain SVG (no chart library needed for one
 * line). The line is scaled to its own min/max so small moves are visible;
 * the min and max are written on the left so the scale stays honest.
 */

interface LineChartProps {
  values: number[]
  /** Formats the min / max labels, e.g. `formatUsd`. */
  format: (value: number) => string
  /** Shown when there are fewer than 2 points. */
  emptyText: string
  /** Accessible description, e.g. "Equity, sim, last 30 days". */
  label: string
}

const WIDTH = 300
const HEIGHT = 90
const PAD = 4

export function LineChart({ values, format, emptyText, label }: LineChartProps) {
  if (values.length < 2) return <p className="panel-muted">{emptyText}</p>

  const min = Math.min(...values)
  const max = Math.max(...values)
  const range = max - min
  const points = values
    .map((value, index) => {
      const x = PAD + (index / (values.length - 1)) * (WIDTH - 2 * PAD)
      // Flat line (range 0): draw it through the middle.
      const ratio = range === 0 ? 0.5 : (value - min) / range
      const y = HEIGHT - PAD - ratio * (HEIGHT - 2 * PAD)
      return `${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')

  return (
    <figure className="chart">
      <div className="chart-scale">
        <span>{format(max)}</span>
        <span>{format(min)}</span>
      </div>
      <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label={label}>
        <polyline points={points} fill="none" stroke="currentColor" strokeWidth="2" />
      </svg>
    </figure>
  )
}
