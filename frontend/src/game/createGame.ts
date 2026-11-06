import { AUTO, Game, Scale } from 'phaser'

import { CANVAS_SIZE, TownScene } from './TownScene'

/**
 * Starts the Phaser game inside `parent`. `Scale.FIT` keeps the square town
 * fully visible at any window size. The caller must `destroy(true)` it.
 */
export function createGame(parent: HTMLElement): Game {
  return new Game({
    type: AUTO,
    parent,
    width: CANVAS_SIZE,
    height: CANVAS_SIZE,
    backgroundColor: '#6a9f4b',
    scale: { mode: Scale.FIT, autoCenter: Scale.CENTER_BOTH },
    scene: [TownScene],
  })
}
