# Metallic future game-system contracts

This folder is **not** the Metallic game engine. It contains dependency-free Node.js 22 data contracts, examples and repeatable tests while the real desktop/VR engine and authoritative server are being chosen and built.

| Folder | Prototype | Production work still missing |
| --- | --- | --- |
| `hub/` | Original outdoor world blueprint, connected POI map and skip-safe cinematic contract | 3D art, lights, animation, collisions, navmesh, gameplay scene |
| `creator/` | Invention category metadata, revision-bound approval and safe public searching | Signed server moderation, content scanning, CDN, assets, creator permissions |
| `ai/` | Roomie bounded fear/bond/mood FSM with fixed animation effects | NPC rigs/navmesh, replicated server actions, optional approved dialogue/voice |
| `learning/` | Four workshop rubrics, host/teacher grants and authoritative-achievement event shape | Teacher verification service, human grading, enrollment, records, integration |
| `watch/` | Original customizable menu model for mouse/gamepad/VR/touch | Actual accessible visual menus and server permission calls |

Run locally: `cd game && npm test`. Support case policy tests: `cd services/support && npm test`.

**Security note:** Passing a forged `authenticatedService: true` object or `approved` content metadata from an untrusted client does not make it trusted. These modules must be called only *after* real middleware authenticates service identities and fetches immutable moderation/ownership state from a server-controlled database.

Documentation: [Meadow world spec](../docs/METALLIC_MEADOW_HUB.md), [creator/AI/education/support spec](../docs/METALLIC_CREATOR_LEARNING_AI.md), [research register](../docs/METALLIC_SYSTEMS_RESEARCH_2026.md), [engine ADR](../docs/adr/ADR-0002-metallic-runtime.md).
