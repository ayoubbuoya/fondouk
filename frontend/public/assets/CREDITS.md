# Art credits

The town is 3D (HANDOFF D76, D77).

- **Buildings, walls, props and the ground** are made in code
  (`src/game3d/buildings.ts`): no files, no licence.
- **Characters:** **KayKit Character Pack: Adventurers 1.0** by
  [Kay Lousberg](https://www.kaylousberg.com),
  https://kaylousberg.itch.io/kaykit-adventurers. License: **CC0 1.0** (see
  `LICENSE-kaykit-adventurers.txt`); credit is optional, we give it anyway.
  Downloaded on 10 Nov 2026 from the creator's GitHub,
  https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0.
  Only the 5 character files are copied, renamed to lowercase:

  | Here | In the pack (`addons/kaykit_character_pack_adventures/`) |
  | --- | --- |
  | `models/characters/knight.glb` | `Characters/gltf/Knight.glb` |
  | `models/characters/barbarian.glb` | `Characters/gltf/Barbarian.glb` |
  | `models/characters/mage.glb` | `Characters/gltf/Mage.glb` |
  | `models/characters/rogue.glb` | `Characters/gltf/Rogue.glb` |
  | `models/characters/rogue_hooded.glb` | `Characters/gltf/Rogue_Hooded.glb` |

- **Portraits:** `characters/<id>/face.png` (128 × 128) are rendered by us from
  the KayKit models above (`src/game3d/portraitStudio.ts`), so they are CC0
  derivatives of that pack.

The first, 2D town used the Ninja Adventure Asset Pack (CC0, by Pixel-boy and
AAA). That art was removed in D77; it is in git history before that commit.
