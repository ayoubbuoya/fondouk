/**
 * The text that floats over the 3D town: village signs, landmark names, and
 * each character's name tag, speech bubble and "LEVEL UP!" text.
 *
 * These are ordinary HTML elements. three's `CSS2DRenderer` moves each one
 * every frame so it sits over its 3D anchor. That keeps text sharp and the
 * same size at any zoom, in the HUD's font, styled in `index.css`
 * (`.town3d-*`).
 *
 * **Security:** names and arena messages come from users and from the LLM.
 * They are only ever written with `textContent`, never `innerHTML`, so they
 * cannot inject markup.
 */

import { CSS2DObject } from 'three/addons/renderers/CSS2DRenderer.js'

import type { Village, YieldPoint } from '../lib/api'
import { formatApy, staleLabel, totalApy } from '../lib/format'
import { sparklinePoints } from './sparkline'

const SPARK_W = 90
const SPARK_H = 26
const SVG_NS = 'http://www.w3.org/2000/svg'

/** Arena messages are cut to this many characters in the bubble (same as the 2D town). */
const BUBBLE_MAX_CHARS = 45

function element<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text = ''): HTMLElementTagNameMap[K] {
  const created = document.createElement(tag)
  created.className = className
  if (text) created.textContent = text
  return created
}

/** A label anchored by its bottom centre, so it grows upwards from its 3D point. */
function anchored(content: HTMLElement): CSS2DObject {
  const object = new CSS2DObject(content)
  object.center.set(0.5, 1)
  return object
}

/** A clickable name over a landmark building. */
export function createLandmarkSign(label: string, onClick: () => void): CSS2DObject {
  const button = element('button', 'town3d-landmark', label)
  button.type = 'button'
  button.addEventListener('click', onClick)
  return anchored(button)
}

/** The sign over a village: stale note, APY, name and the 7-day line. */
export class VillageSign {
  readonly object: CSS2DObject

  private readonly root = element('button', 'town3d-sign')
  private readonly stale = element('span', 'town3d-sign-stale')
  private readonly apy = element('span', 'town3d-sign-apy')
  private readonly name = element('span', 'town3d-sign-name')
  private readonly line = document.createElementNS(SVG_NS, 'polyline')

  constructor(onClick: () => void) {
    this.root.type = 'button'
    this.root.addEventListener('click', onClick)

    const spark = document.createElementNS(SVG_NS, 'svg')
    spark.setAttribute('class', 'town3d-sign-spark')
    spark.setAttribute('viewBox', `0 0 ${SPARK_W} ${SPARK_H}`)
    spark.setAttribute('width', String(SPARK_W))
    spark.setAttribute('height', String(SPARK_H))
    spark.append(this.line)

    this.root.append(this.stale, this.apy, this.name, spark)
    this.object = anchored(this.root)
  }

  /** Redraws the sign; called on every APY update and once a minute (stale age). */
  update(village: Village, history: YieldPoint[], now: Date): void {
    this.apy.textContent = formatApy(totalApy(village))
    this.name.textContent = village.name
    const stale = staleLabel(village, now)
    this.stale.textContent = stale ?? ''
    this.stale.hidden = !stale
    this.root.classList.toggle('is-stale', village.stale)

    const values = history.map((point) => totalApy({ apy_base: point.apy_base, apy_reward: point.apy_reward }) ?? 0)
    this.line.setAttribute('points', sparklinePoints(values, SPARK_W, SPARK_H))
  }
}

/**
 * Everything over one character's head, stacked from the head up:
 *
 * ```text
 *      LEVEL UP!        <- floats up and fades (2.2 s)
 *   ╭─────────────╮
 *   │ speech text │     <- shown for about 5 s
 *   ╰─────────────╯
 *      Amira 4          <- the name tag, lifted by its tier (see `layoutTagTiers`)
 *        ☺
 * ```
 */
export class AvatarLabel {
  readonly object: CSS2DObject

  private readonly stack = element('div', 'town3d-avatar')
  private readonly tag = element('button', 'town3d-tag')
  private bubble?: HTMLElement

  constructor(text: string, onClick: () => void) {
    this.tag.type = 'button'
    this.tag.textContent = text
    this.tag.addEventListener('click', onClick)
    this.stack.append(this.tag)
    this.object = anchored(this.stack)
  }

  setText(text: string): void {
    this.tag.textContent = text
  }

  /** Lifts the tag by `tier` steps, so neighbours in a row do not print over each other. */
  setTier(tier: number): void {
    this.stack.style.setProperty('--tier', String(tier))
  }

  /** Shows a speech bubble; a new one replaces the old one. It removes itself when its CSS animation ends. */
  say(text: string): void {
    this.bubble?.remove()
    const shown = text.length > BUBBLE_MAX_CHARS ? `${text.slice(0, BUBBLE_MAX_CHARS)}…` : text
    const bubble = element('div', 'town3d-bubble', shown)
    bubble.addEventListener('animationend', () => {
      bubble.remove()
      if (this.bubble === bubble) this.bubble = undefined
    })
    this.bubble = bubble
    this.stack.prepend(bubble)
  }

  /** Floats a short gold text up from the tag (a new level or badge, D74). */
  celebrate(text: string): void {
    const floating = element('div', 'town3d-celebration', text)
    floating.addEventListener('animationend', () => floating.remove())
    this.stack.append(floating)
  }

  /**
   * Takes the label out of the scene. Needed explicitly: three only removes
   * the HTML of a label when the label itself is removed, not when its
   * parent is.
   */
  dispose(): void {
    this.object.removeFromParent()
  }
}
