# Extraction coverage / progress

Owner's own 2026 Rec Room build (20260323.14, Unity 6000.0.27f1, IL2CPP). All extracted/decrypted
data is LOCAL (gitignored); this document records only our own counts and analysis.

Two distinct data domains:

## A. Addressables bundles (unencrypted) — `StreamingAssets/aa/StandaloneWindows64`
- **2,787 bundles (1.6 GB)**. Catalog `catalog.bin`: **16,346 keys → 23,265 locations**.
- Catalog addresses by resource type (from `unity-catalog`):
  Texture2D 5,511 · Sprite 5,048 · GameObject 4,361 · AudioClip 1,161 · Material 920 ·
  Localization StringTable 597 · AvatarItemMaterial 455 · EquipmentSkin 434 · Mesh 366 ·
  ThemeStyle 346 · Shader 123 · PlayerAnimationAsset 114 · Avatar 103 · AnimationClip 90 · (rest UI/theming/avatar).
- Bundle scan (`unity-scan`, all 2,787): **every bundle parses fully; 932,336 objects, 100% value
  trees read.** Top object types: MonoBehaviour 373,069 · GameObject 207,879 · RectTransform 116,992 ·
  Transform 90,887 · MeshFilter 10,473 · BoxCollider 10,249 · MeshRenderer 9,826 · Texture2D 5,280 ·
  Mesh 3,229 · SkinnedMeshRenderer 2,742 · Material 2,300 · AudioClip 1,178 · AnimationClip 718.
- Typed reader success (ok / fail):
  GameObject 207,879/0 · Transform 90,887/0 · MeshFilter 10,473/0 · MeshRenderer+Skinned 12,568/0 ·
  Material 2,300/0 · **Mesh 3,216/13** · **Texture2D 5,266/14** · AssetBundle 2,787/0.
- **Reader failures: 13 of 932,336 objects (0.0014%)**, one remaining cause:
  1. `13× compressed mesh` (`m_MeshCompression != 0`) — packed-vertex meshes (`m_CompressedMesh`
     PackedBitVectors); reader gap, ~150 lines of PackedBitVector decode to close. FOLLOW-UP.
  2. ~~`14× zero-sized texture`~~ — FIXED: `decode_rgba8` now returns an empty image for 0×0
     (streaming placeholders), so Texture2D typed extraction is **5,280 / 0**.
- Batch extraction already exists two ways: our `unity-assets` readers (99.9986% of objects) and a
  prior AssetStudioModCLI export in `decomp-full/assets/AssetStudioExport` (386,575 files: 9,050
  textures→PNG, 3,225 meshes→OBJ, 1,178 audio→WAV, 373k MonoBehaviours→JSON).

## B. Built-in base scenes (encrypted) — `RecRoom_Data`
- Encrypted with an 8-byte `00 00 52 52` marker + a per-file stream cipher applied by the
  **Referee.dll anti-cheat** (code-virtualized; see `DECRYPTION_RE_STATUS.md`). Not reimplemented;
  decrypted via the runtime oracle into a local plaintext **corpus** (`rebuild_corpus.py`).
- **337 marker-encrypted files (1.83 GB)**: `globalgamemanagers`, `globalgamemanagers.assets`,
  114 `levelN` scenes, `resources.assets`, 114 `sharedassets*.assets`, 33 `levelN.resS`, 71 `.resource`.
  (Another 108 install files — e.g. `resources.assets.resS` (1.3 GB), `sharedassets*.assets.resS` —
  are plaintext and read directly, no decryption.)
- **Scene coverage: 114 / 114 built-in scenes decrypt and parse** (`unity-levels <corpus>` →
  all 114 with scene path, roots, per-class counts). Every built-in room/map is readable:
  dormroom2, reccenter, charades/the-ink-space, discgolf (+Propulsion), dodgeball, paddleball,
  eventroom, paintball (River/Construction/Castle/ClearCut/Dam/DriveIn/MerryMidway/Lobby),
  Quest Goblin_1 + Scifi_1, Orientation, StuntRunner, RecRally, UGC dorms, etc.
- Corpus completeness: **337 / 337 encrypted files decrypted** (scenes + `.assets` +
  `globalgamemanagers.assets` + encrypted `.resS`/`.resource` geometry/texture/audio streams), via
  `rebuild_corpus.py` (runtime oracle, robust/resumable). State in `assets/decrypted/corpus-manifest.json`.

## C. IL2CPP code decompile
- Complete dump exists locally (`decomp-full/code`): `dump.cs` (409,503 methods, all 419 images),
  `il2cpp.h`, `script.json`, `DummyDll/` (420 assemblies). Mapped in `decomp-full/README.md`
  (gameplay/Circuits, Tachyon realtime handshake, etc.). Reference-only; not in the repo.

## What's left / gaps
1. **Drive down the 27 reader failures** (optional): add a compressed-mesh (packed vertex) path to
   the Mesh reader (13); treat zero-sized Texture2D as empty rather than an error (14).
2. **Per-environment load data (#3)**: scene-map + per-scene externals/lighting manifest from the
   corpus, consumed by the client (the `maps` session is wiring client loading).
3. Encrypted `.resS`/`.resource` decryption to 337/337 (in progress via `rebuild_corpus.py`) so
   full mesh/texture geometry of the base scenes is available (bundle-sourced meshes already export).

## Headline numbers
- Addressables objects readable: **932,323 / 932,336 (99.9986%)** (only 13 compressed meshes left).
- Built-in scenes decrypted + parsed: **114 / 114 (100%)**.
- Encrypted base files in corpus: **337 / 337 (100%)**.
