# Metallic-RC — real 2026 map coverage

The Rust client loads every built-in Rec Room 2026 environment straight from the decrypted base scenes (the `levelN`
SerializedFiles), with their own geometry, materials, colliders and realtime lights. `world::load`/`levels::load_scene`
read the `.assets` from the decrypted corpus and the out-of-line `.resS`/`.resource` streams from the game install
(plaintext there) or the corpus (for the streams that were encrypted on disk). The Dorm keeps its existing bridged
look (2019 layout + 2026 Addressables prefabs); every other map is the authentic 2026 scene.

Verified 2026-11-10 against build 23191908 (Unity 6000.0.27f1). Loads measured with the offscreen rasterizer
(`cargo run -p metallic-rc-client --example rr-rasterize -- <scene> out.png`). "problems" are refs the builder could
not resolve or decode; they are logged and skipped, never fatal.

## Playable environments — all load with full geometry + materials

| Room (server name) | scene / levelN | parts | materials | lights | colliders | problems |
|---|---|---:|---:|---:|---:|---|
| DormRoom | dormroom2 / level6 | 82 | 40 | 8 | 134 | 0 |
| RecCenter | reccenter / level7 | 477 | 86 | 40 | 1295 | 0 |
| Charades | charades / level8 | 114 | 30 | 43 | 317 | 0 |
| DiscGolf | discgolf / level9 | 693 | 27 | 30 | 1305 | 0 |
| Dodgeball | dodgeball / level11 | 179 | 24 | 23 | 113 | 0 |
| EventRoom | eventroom / level12 | 263 | 56 | 26 | 660 | 0 |
| Paddleball | paddleball / level13 | 200 | 29 | 28 | 75 | 0 |
| Paintball River | paintball / level14 | 969 | 53 | 6 | 2061 | 0 |
| Paintball Castle | Paintball_Castle / level16 | 1116 | 56 | 14 | 2191 | 0 |
| Paintball Dam | Paintball_Dam / level18 | 1919 | 72 | 145 | 3468 | 0 |
| Quest: Foyer | Quest_Foyer / level19 | 301 | 46 | 40 | 1867 | 1* |
| Quest: Library | Quest_Library / level26 | 1087 | 34 | 56 | 2229 | 1* |
| Quest: Cafeteria | Quest_Cafeteria / level29 | 279 | 34 | 62 | 830 | 1* |
| Quest: Sci-Fi Foyer | Quest_Scifi_Foyer / level38 | 432 | 49 | 15 | 1711 | 0 |
| Orientation: Practice Gym | Orientation_PracticeGym / level34 | 246 | 55 | 21 | 842 | 0 |
| StuntRunner | level100 | 47 | 6 | 5 | 39 | 0 |
| Drive-In | level103 | 2745 | 133 | 44 | 4576 | 0 |
| Merry Midway | level104 | 2934 | 141 | 61 | 1557 | 0 |
| Rec Rally | level107 | 1802 | 95 | 64 | 3872 | 0 |

The remaining built-in maps of the same families load the same way (Disc Golf Propulsion, Paintball Construction/
Clear Cut, the rest of the Quest Goblin and Sci-Fi rooms, the other Orientation scenes, the ink-space scenes).
Screenshots of the Dorm, Rec Center and Paintball River confirm the look is real geometry + textured materials, not a
grey stand-in.

Drive-In / Merry Midway / Rec Rally (level103/104/107) stream their geometry from `levelN.resS` that is **encrypted on
disk**; they load fully only because the decryption session's corpus now holds the decrypted copies, and the resolver
reads the corpus first. Without the decrypted corpus those three (and the other ~30 encrypted-`.resS` scenes) would
load layout + prefab geometry but miss the base-scene-embedded meshes/textures.

## Known gaps (minor, non-fatal)

- **`* ` compressed decorative meshes** — e.g. `CandleFlameMesh` in the Quest rooms. `level::mesh` does not yet
  decode Unity mesh compression (`m_MeshCompression != 0`), so a handful of small props are skipped (one mesh per
  affected room). Cosmetic; the room is otherwise complete.
- **Baked lightmaps** — `LightmapSettings` has a Unity 6 layout change (+12 bytes) not yet parsed, so rooms light from
  their realtime/mixed lights + ambient rather than baked lightmaps. Rooms look correct, just without baked GI.
- **Cubemaps (reflection probes / skybox)** — the Unity 6 Cubemap (class 89) layout is not verified; reflection/skybox
  cubemaps may not load. Does not affect geometry or materials.

## Not offered as rooms (internal scenes)

main_root (level0), empty (level1), late_main_root (level2), TitleScreen (level3), LogoutScene (level4),
RecRoomStudioTerrainScene (level5), and the `*_additive`/bootstrap scenes are engine/flow scaffolding, not player
environments, so matchmaking does not list them.
