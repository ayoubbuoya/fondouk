/**
 * The "camera filters" of the 3D town (D78). Instead of drawing the scene
 * straight to the screen, each frame goes through a short chain:
 *
 * ```text
 *   scene ─► RenderPass ─► UnrealBloomPass ─► vignette ─► OutputPass ─► screen
 *            the picture   bright things      darker      tone mapping
 *            (off screen)  get a glow         corners     + screen colours
 * ```
 *
 * The off-screen picture stores colours as half-floats, which can be
 * **brighter than white**. That is what makes the glow selective: a lantern's
 * material shines at several times white (`buildings.ts`), sunlit plaster
 * stays below `BLOOM.threshold`, so only the lanterns bloom.
 *
 * Everything here ships inside `three` (`three/addons`): no new dependency.
 */

import { HalfFloatType, Vector2, WebGLRenderTarget, type Camera, type Scene, type WebGLRenderer } from 'three'
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js'
import { OutputPass } from 'three/addons/postprocessing/OutputPass.js'
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js'
import { ShaderPass } from 'three/addons/postprocessing/ShaderPass.js'
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js'
import { VignetteShader } from 'three/addons/shaders/VignetteShader.js'

/** `threshold` is in "times white": only colours brighter than it glow. */
const BLOOM = { strength: 0.6, radius: 0.55, threshold: 1 } as const
/** `offset` below 1 keeps the corners lighter; `darkness` 1 fades towards black. */
const VIGNETTE = { offset: 0.85, darkness: 1 } as const
/**
 * Smooth edges. The renderer's own `antialias` only works when it draws
 * straight to the screen, so the off-screen picture needs its own samples.
 */
const MSAA_SAMPLES = 4

export interface PostFx {
  /** Draws one frame through the chain. Call instead of `renderer.render`. */
  render(): void
  /** Call when the canvas changes size (CSS pixels, like `renderer.setSize`). */
  setSize(width: number, height: number): void
  /** Frees the off-screen pictures and the filter programs. */
  dispose(): void
}

/**
 * Builds the filter chain for `renderer`. The renderer's pixel ratio and tone
 * mapping must be set **before** this is called: the chain reads both.
 */
export function createPostFx(renderer: WebGLRenderer, scene: Scene, camera: Camera): PostFx {
  // 1×1 for now: the first `setSize` gives it the real size.
  const target = new WebGLRenderTarget(1, 1, { type: HalfFloatType, samples: MSAA_SAMPLES })
  const composer = new EffectComposer(renderer, target)

  const bloom = new UnrealBloomPass(new Vector2(1, 1), BLOOM.strength, BLOOM.radius, BLOOM.threshold)
  const vignette = new ShaderPass(VignetteShader)
  vignette.uniforms.offset.value = VIGNETTE.offset
  vignette.uniforms.darkness.value = VIGNETTE.darkness
  // Last: it turns the bright, linear picture into what a screen can show.
  const output = new OutputPass()

  composer.addPass(new RenderPass(scene, camera))
  composer.addPass(bloom)
  composer.addPass(vignette)
  composer.addPass(output)

  return {
    render: () => composer.render(),
    setSize: (width, height) => composer.setSize(width, height),
    dispose() {
      bloom.dispose()
      vignette.dispose()
      output.dispose()
      composer.dispose()
    },
  }
}
