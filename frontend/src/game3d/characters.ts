/**
 * The 3D body of one character: a rigged glTF model with an idle and a walk
 * animation (KayKit Adventurers, see `public/assets/CREDITS.md`).
 *
 * ```text
 *   new CharacterBody(sprite)
 *     ├─ shows a simple stand-in figure at once
 *     └─ loadModel(model) ── ok ──► swap the stand-in for a tinted clone of the model
 *                          └ fail ─► keep the stand-in (missing file, bad file)
 * ```
 *
 * Each model file is downloaded once and then cloned per character
 * (`SkeletonUtils.clone`: a plain `clone()` would share one skeleton, and
 * every copy would move together).
 */

import {
  AnimationMixer,
  Box3,
  CapsuleGeometry,
  Color,
  Group,
  Mesh,
  MeshStandardMaterial,
  SphereGeometry,
  Vector3,
  type AnimationAction,
  type AnimationClip,
  type Material,
  type Object3D,
} from 'three'
import { GLTFLoader, type GLTF } from 'three/addons/loaders/GLTFLoader.js'
import { clone as cloneSkinned } from 'three/addons/utils/SkeletonUtils.js'

import { modelUrl, SPRITE_LOOKS, type CharacterModel, type SpriteId } from '../game/sprites'

/** How tall a character stands, in tiles. Larger than life so it reads from far away. */
export const CHARACTER_HEIGHT = 1.5

/** Seconds to blend between standing and walking. */
const BLEND_SECONDS = 0.2

const loader = new GLTFLoader()
const models = new Map<CharacterModel, Promise<GLTF | null>>()

/** Downloads a model once; `null` when the file is missing or broken. */
function loadModel(model: CharacterModel): Promise<GLTF | null> {
  let pending = models.get(model)
  if (!pending) {
    pending = loader.loadAsync(modelUrl(model)).catch((error: unknown) => {
      // One line per model, not per character: every clone shares this promise.
      console.warn(`3D model "${model}" did not load; using the stand-in figure`, error)
      return null
    })
    models.set(model, pending)
  }
  return pending
}

/**
 * The KayKit clips we play. Exact names: a loose search for "idle" would find
 * `2H_Melee_Idle` (a fighting stance) first, because it comes earlier in the file.
 */
const IDLE_CLIP = 'Idle'
const WALK_CLIP = 'Walking_A'

function findClip(clips: AnimationClip[], name: string): AnimationClip | undefined {
  return clips.find((clip) => clip.name === name)
}

/**
 * Each KayKit model file carries every weapon and shield of its class as a
 * separate mesh (the Knight has 5 shields and 2 swords). Merchants in a souk
 * carry none, so these meshes are removed. Hats, capes and helmets stay.
 */
const GEAR_MESH = /shield|sword|axe|knife|crossbow|wand|staff|spellbook|mug|throwable/i

function removeGear(model: Object3D): void {
  const gear: Object3D[] = []
  // Collect first: removing while `traverse` walks the tree would skip children.
  model.traverse((child) => {
    if (child instanceof Mesh && GEAR_MESH.test(child.name)) gear.push(child)
  })
  for (const mesh of gear) mesh.removeFromParent()
}

const STAND_IN_ROBE = new CapsuleGeometry(0.28, 0.55, 4, 10)
const STAND_IN_HEAD = new SphereGeometry(0.22, 12, 10)
const SKIN = new MeshStandardMaterial({ color: 0xd9a273, roughness: 0.9, flatShading: true })
const TURBAN = new MeshStandardMaterial({ color: 0x2e8c84, roughness: 0.9, flatShading: true })
/** A white tint means "model colours unchanged"; on the stand-in that would be a blank robe. */
const STAND_IN_DEFAULT_ROBE = 0xe9dcc0

export class CharacterBody {
  /** Add this to the scene; move and turn it to move the character. */
  readonly root = new Group()

  private readonly standIn = new Group()
  private readonly ownMaterials: Material[] = []
  private mixer?: AnimationMixer
  private idle?: AnimationAction
  private walk?: AnimationAction
  private walking = false
  private disposed = false
  /** Drives the stand-in's little hop while it walks. */
  private stride = 0

  constructor(sprite: SpriteId) {
    const look = SPRITE_LOOKS[sprite]
    this.buildStandIn(look.tint)
    void loadModel(look.model).then((gltf) => {
      if (gltf && !this.disposed) this.useModel(gltf, look.tint)
    })
  }

  /** Switches between the idle and the walk animation. */
  setWalking(walking: boolean): void {
    if (walking === this.walking) return
    this.walking = walking
    const [from, to] = walking ? [this.idle, this.walk] : [this.walk, this.idle]
    from?.fadeOut(BLEND_SECONDS)
    to?.reset().fadeIn(BLEND_SECONDS).play()
    if (!walking) {
      this.standIn.position.y = 0
      this.standIn.rotation.z = 0
    }
  }

  /** Call once per frame with the seconds since the last frame. */
  update(deltaSeconds: number): void {
    this.mixer?.update(deltaSeconds)
    if (this.walking && this.standIn.parent) {
      this.stride += deltaSeconds * 11
      this.standIn.position.y = Math.abs(Math.sin(this.stride)) * 0.1
      this.standIn.rotation.z = Math.sin(this.stride) * 0.08
    }
  }

  /** Frees what this character alone owns (its tinted materials). */
  dispose(): void {
    this.disposed = true
    this.mixer?.stopAllAction()
    for (const material of this.ownMaterials) material.dispose()
    this.root.removeFromParent()
  }

  /** A robed figure with a turban: visible at once, and the fallback if the model fails. */
  private buildStandIn(tint: number): void {
    const robeMaterial = new MeshStandardMaterial({
      color: tint === 0xffffff ? STAND_IN_DEFAULT_ROBE : tint,
      roughness: 0.9,
      flatShading: true,
    })
    this.ownMaterials.push(robeMaterial)

    const robe = new Mesh(STAND_IN_ROBE, robeMaterial)
    robe.position.y = 0.56
    const head = new Mesh(STAND_IN_HEAD, SKIN)
    head.position.y = 1.2
    const turban = new Mesh(STAND_IN_HEAD, TURBAN)
    turban.scale.set(1.15, 0.6, 1.15)
    turban.position.y = 1.36
    for (const mesh of [robe, head, turban]) {
      mesh.castShadow = true
      this.standIn.add(mesh)
    }
    this.root.add(this.standIn)
  }

  private useModel(gltf: GLTF, tint: number): void {
    const model = cloneSkinned(gltf.scene)
    // Before measuring: a raised sword would make the body count as taller.
    removeGear(model)
    this.fitHeight(model)
    this.tint(model, tint)

    this.mixer = new AnimationMixer(model)
    const idleClip = findClip(gltf.animations, IDLE_CLIP)
    const walkClip = findClip(gltf.animations, WALK_CLIP)
    this.idle = idleClip && this.mixer.clipAction(idleClip)
    this.walk = walkClip && this.mixer.clipAction(walkClip)
    // The character may already be walking when its model arrives.
    ;(this.walking ? this.walk : this.idle)?.play()

    this.standIn.removeFromParent()
    this.root.add(model)
  }

  /** Scales the model so it is `CHARACTER_HEIGHT` tall, whatever units its file uses. */
  private fitHeight(model: Object3D): void {
    const size = new Box3().setFromObject(model).getSize(new Vector3())
    if (size.y > 0) model.scale.multiplyScalar(CHARACTER_HEIGHT / size.y)
  }

  /**
   * Multiplies the tint into the model's colours. Materials are cloned first:
   * clones of one model share materials, so tinting in place would recolour
   * every character on that model.
   */
  private tint(model: Object3D, tint: number): void {
    const tintColor = new Color(tint)
    model.traverse((child) => {
      if (!(child instanceof Mesh)) return
      child.castShadow = true
      if (tint === 0xffffff) return
      const tinted = (Array.isArray(child.material) ? child.material : [child.material]).map((material: Material) => {
        const copy = material.clone()
        if (copy instanceof MeshStandardMaterial) copy.color.multiply(tintColor)
        this.ownMaterials.push(copy)
        return copy
      })
      child.material = Array.isArray(child.material) ? tinted : tinted[0]
    })
  }
}
