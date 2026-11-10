import { AUTO, Game, Scale } from 'phaser'

import { TownScene } from './TownScene'

/**
 * Starts the Phaser game inside `parent`. `Scale.RESIZE` makes the canvas
 * the same size as `parent` (1 canvas pixel = 1 screen pixel), so the town
 * fills the panel with no bars; the scene's camera decides what is visible.
 * The caller must `destroy(true)` it.
 */
export function createGame(parent: HTMLElement): Game {
  return new Game({
    type: AUTO,
    parent,
    // The sand colour, in case a frame shows before the ground is drawn.
    backgroundColor: '#f6a55c',
    scale: { mode: Scale.RESIZE, width: '100%', height: '100%' },
    scene: [TownScene],
  })
}
