/**
 * Camera zoom levels for the town map. Pure, no Phaser: unit-tested.
 *
 * The canvas fills its panel (`Scale.RESIZE`), so the smallest useful zoom
 * depends on the panel size: see `zoomLimits`.
 */

export const MAX_ZOOM = 3
/** One wheel notch multiplies or divides the zoom by this. */
export const ZOOM_STEP = 1.15

export interface Size {
  w: number
  h: number
}

export interface ZoomLimits {
  /** The camera cannot zoom out past this: the world would not fill the view. */
  min: number
  /** The zoom the town opens with. */
  start: number
}

/**
 * Zoom limits for a view of `view` pixels.
 *
 * - `min` = the world just covers the view (no empty bars around the map);
 * - `start` = the whole town is visible, with the desert margin filling the
 *   rest; never below `min`, never above `MAX_ZOOM`.
 *
 * ```text
 *   wide view                    tall view
 *   ┌──desert──────────┐         ┌─────────┐
 *   │   ┌──town──┐     │         │▒┌─town─┐│  the town is cut left and
 *   │   └────────┘     │         │▒└──────┘│  right: `min` wins, the user
 *   └──────────────────┘         └─────────┘  can pan
 * ```
 */
export function zoomLimits(view: Size, world: Size, town: Size): ZoomLimits {
  const min = Math.min(MAX_ZOOM, Math.max(view.w / world.w, view.h / world.h))
  const wholeTown = Math.min(view.w / town.w, view.h / town.h)
  return { min, start: Math.min(MAX_ZOOM, Math.max(min, wholeTown)) }
}

/**
 * The zoom after one wheel event. A negative `wheelDeltaY` (wheel pushed
 * away, or a trackpad pinch-out) zooms in, a positive one zooms out, and 0
 * keeps the zoom unchanged. Only the direction is used, not the size, so a
 * mouse and a trackpad feel the same per event.
 */
export function nextZoom(current: number, wheelDeltaY: number, minZoom: number): number {
  if (wheelDeltaY === 0) return current
  const zoom = wheelDeltaY < 0 ? current * ZOOM_STEP : current / ZOOM_STEP
  return Math.min(MAX_ZOOM, Math.max(minZoom, zoom))
}
