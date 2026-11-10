/**
 * The 7-day APY line on a village sign (F2.3), as SVG points. Pure, no DOM:
 * unit-tested.
 */

/**
 * Scales `values` into a `width` × `height` box, as the `points` attribute of
 * an SVG `<polyline>`. The line uses its own min and max, so small moves are
 * visible; a flat history is a line through the middle.
 *
 * @returns `""` for fewer than 2 values: there is no line to draw.
 */
export function sparklinePoints(values: number[], width: number, height: number): string {
  if (values.length < 2) return ''
  const min = Math.min(...values)
  const max = Math.max(...values)
  const range = max - min
  return values
    .map((value, index) => {
      const x = (index / (values.length - 1)) * width
      const ratio = range === 0 ? 0.5 : (value - min) / range
      // SVG y grows downwards: the highest value is at the top (y = 0).
      const y = height - ratio * height
      return `${round(x)},${round(y)}`
    })
    .join(' ')
}

/** One decimal is enough for a 90 px wide line and keeps the markup short. */
function round(value: number): number {
  return Math.round(value * 10) / 10
}
