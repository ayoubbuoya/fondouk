/**
 * Camera zoom levels for the town map. Pure, no Phaser: unit-tested.
 *
 * Zoom 1 shows the whole town (what `Scale.FIT` already fits in the panel),
 * so the camera can never zoom out past it: there is nothing more to see.
 */

export const MIN_ZOOM = 1
export const MAX_ZOOM = 3
/** One wheel notch multiplies or divides the zoom by this. */
export const ZOOM_STEP = 1.15

/**
 * The zoom after one wheel event. A negative `wheelDeltaY` (wheel pushed
 * away, or a trackpad pinch-out) zooms in, a positive one zooms out, and 0
 * keeps the zoom unchanged. Only the direction is used, not the size, so a
 * mouse and a trackpad feel the same per event.
 */
export function nextZoom(current: number, wheelDeltaY: number): number {
  if (wheelDeltaY === 0) return current
  const zoom = wheelDeltaY < 0 ? current * ZOOM_STEP : current / ZOOM_STEP
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom))
}
