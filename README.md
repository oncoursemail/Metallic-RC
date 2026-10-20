# Metallic-RC

**Project naming:** [Metallic-RC / Metallic-MC / Metallic-RB / Metallic-PX](docs/PROJECT_IDENTITIES.md). Family domain: **Metallic.club** (application routing is not configured by this repository change).

> **Project status: Private native desktop playtest in development.**
> Metallic Meadow now has a runnable desktop client and authoritative multiplayer session: explore three trails, share their progress, and ring the barn bell. This development build is unsigned. The full social, VR, account, voice, and creator platform remains in development or planned; its release gates are unchanged.

[![Roadmap & progress](https://img.shields.io/badge/Roadmap-Live%20progress-24292f?logo=github)](docs/PROGRESS.md)
[![Project plan](https://img.shields.io/badge/Plan-Engineering%20roadmap-323f53)](docs/ROADMAP.md)

![Verified Metallic engineering progress](docs/progress.svg)

*This live artwork represents completed GitHub engineering issues, **not** available game builds or real player counts.*

[Original development-status website prototype](docs/site/index.html) · [How to preview the website locally](docs/site/README.md)

## Play Metallic Meadow

The [native playtest guide](game/native/README.md) covers the desktop build, controls, local hosting, joining a known server, and dedicated-server startup. The original Meadow includes a barn, orchard, treehouse setting, and little green. Its playable activity is a shared three-landmark welcome trail followed by the barn bell; golf and climbing are scenery at this stage.

- **Desktop client:** third-person movement, jumping, animated visitors, host/join menus, live watch map, shared objectives, and saved input/audio preferences.
- **Native authority:** fixed 30 Hz movement and interactions, bounded input, room capacity, live-session reconnect reservations, compact snapshots, and validated activity persistence.
- **Private session runner:** verifies local build hashes, supervises one loopback server with bounded restart, and records readiness. See [runner setup](services/session-runner/README.md).
- **Evidence and scope:** [native playtest validation](docs/NATIVE_PLAYTEST_VALIDATION.md) and the [provisional engine experiment](docs/ADR_NATIVE_PLAYTEST.md). A local test receipt does not replace hosted CI or public-release acceptance.

Build Windows x64, Linux x64, and Linux server packages with [scripts/build_native.py](scripts/build_native.py). Every package carries its source revision, source digest, executable/pack hashes, redistribution notices, and explicit unsigned development status.

## What we are building

Metallic-RC is intended to be an independent, high-performance social game platform with its own client, dedicated servers, launcher, identity service, creator tools, user-generated rooms, visual scripting, and optional integrations with cooperating community projects.

### Product pillars

1. **Cross-device multiplayer:** PC first; VR alongside a shared multiplayer vertical slice; Linux, Android, and iOS after platform validation.
2. **Player-controlled creation:** in-game building, modular prefabs, an asset pipeline, and a next-generation *Circuits V3* typed node graph.
3. **Social platform:** accounts, profiles, friends, parties, rooms, matchmaking, spatial voice, moderation, reporting, and search.
4. **Creator economy:** moderated marketplace, entitlements, auditable token ledger, creator analytics, and optional premium upgrades.
5. **Reliable infrastructure:** authoritative sessions, performance budgets, monitoring, anti-abuse, backups, and defensive network protection.
6. **Open integrations:** an optional, permissioned federation protocol to expose public status and launch other participating projects.

## Live project tracking

- **[Roadmap and milestones](docs/ROADMAP.md)** — scopes, dependencies, test gates, order of delivery.
- **[Live progress dashboard](docs/PROGRESS.md)** — automatically generated from repository GitHub issues; reflects *closed issues*, not claims that binaries are already usable.
- **[Architecture and research guide](docs/ARCHITECTURE.md)** — component boundaries, decisions, performance, and research.
- **[Playable native Meadow](game/native/README.md)** — desktop controls, shared activity, private multiplayer, persistence, and build instructions.
- **[Launcher component setup](services/launcher/README.md)** — independent game and hosting choices, saved preferences, and installation planning; signed native installers remain pending.
- **[Metallic Meadow social hub](docs/METALLIC_MEADOW_HUB.md)** — original barn/farm/golf/treehouse world plan, cinematic storyboard, navigation manifest, and accessibility gates.
- **[Creator, teachers, AI companions, achievements, watch and support](docs/METALLIC_CREATOR_LEARNING_AI.md)** — implementation specs and acceptance gates.
- **[Game system contracts and tests](game/README.md)** — standalone system-contract prototypes, separate from the native client and from deployed APIs.
- **[World, Academy, creator exchange and AI research](docs/METALLIC_WORLD_ACADEMY_AI_RESEARCH.md)** — supporting design research and staged delivery requirements.
- **[Global multiplayer, anti-cheat, Linux and infrastructure research](docs/METALLIC_GLOBAL_INFRASTRUCTURE_SECURITY_2026.md)** — regional session fleet, transport, server authority, performance objectives, and security proposals.
- **[Hypothetical capacity planner](tools/capacity_planner.py)** and [10k-player assumption scenario](ops/capacity/hypothetical_10k_multi_region.json) — sizing math; not measured deployed-server capacity.
- **[Reference authoritative input gate](prototypes/authority/input_gate.py)** — cross-platform policy prototype with tests; not integrated into game transport or a complete anti-cheat.
- **[Metallic Atlas backend lab](services/metallic-control/README.md)** — original Go node heartbeat API, pressure-aware reservations and read-only Linux agent with race/HTTP tests; **not deployed, not a real game server**.
- **[Owned hardware and custom infrastructure research](docs/METALLIC_OWN_INFRASTRUCTURE_2026.md)** — observed constraints, workload isolation, advanced server design, cross-device/network goals, and staged deployment safety.
- **[Control-plane API schema](services/metallic-control/openapi.json)** — versioned lab operator/agent API contract; separate from player-facing services.
- **[240-feature capability catalog](docs/FEATURE_CATALOG.md)** — concrete feature candidates and suggested acceptance tests.
- **[Research backlog](docs/RESEARCH.md)** — sources, experiments, unknowns, and competitor review.
- **[Contributor development rules](docs/DEVELOPMENT.md)** — issue labels, PR review gates and evidence requirements.
- **[Visual identity and UI quality system](docs/DESIGN_SYSTEM.md)** — professional interface direction, motion, VFX and anti-generic acceptance checks.
- **[AI players and stress-testing laboratory](docs/QUALITY_AND_STRESS_TESTING.md)** — staged load, synthetic protocol users, game AI and measurable release gates.
- **[Moderation, Discord and staff titles](docs/MODERATION_DISCORD_STAFF.md)** — unified permissions, in-game and web consoles, bot design, Volunteer Mod / Developer title controls.
- **[Trusted player-count aggregator service](services/player-count/README.md)** — authenticated, de-duplicated heartbeat aggregation with separate synthetic-actor tracking (local code only; no live count).
- **[Meta Quest release preparation](docs/META_QUEST_RELEASE.md)** — Horizon Store, channels, ARM64 build quality and verification.
- **[Live website prototype](docs/site/index.html)** — handcrafted responsive layout reading the generated aggregate progress JSON.
- **[Tracked work items](../../issues)** — one issue per verifiable deliverable.
- **[Pull requests](../../pulls)** — proposed changes and review history.

The `Update project progress` GitHub Actions workflow refreshes the Markdown dashboard after relevant issue/PR changes, on demand, and daily. Issues use `[P0]` to `[P6]` title prefixes; the updater counts open/closed issues in each stage and refuses to fabricate implementation progress. An issue should close only when its linked acceptance tests have passed.

## Full first-playable acceptance target

**First playable vertical slice:** a signed desktop test build connects to an independently hosted authoritative server, joins a room with two players, replicates movement, shows a basic avatar, sends optional positional voice, and survives reconnects. The private native experiment now exercises movement, avatars, authoritative shared activity, and reconnects. Signing, independently hosted account/ticket admission, voice, and VR/device acceptance are still required before this broader milestone can close.

## Platform and tech candidates

- **Client:** Godot 4.5.2 powers the private desktop experiment. Unity/OpenXR versus Godot/OpenXR remains a separate measured production decision.
- **Backend:** the experiment uses native Godot ENet authority; the existing Go Atlas lab covers control-plane reservations. Production session architecture, modular APIs, PostgreSQL, Valkey, object storage, and OpenTelemetry remain proposals.
- **Website + launcher:** TypeScript web console, Tauri/Rust desktop launcher candidate.
- **Creator runtime:** versioned graph schema, deterministic/server-authoritative execution for trusted game logic.
- **Infrastructure:** self-owned machines for dev/test; provider-protected networking for public UDP sessions. Cloudflare's regular HTTP proxy is not a complete UDP game-server DDoS solution.

The native experiment has a scoped ADR. It does not settle the production engine, public fleet, or VR architecture.

## Principles

- Report **planned / in progress / blocked / tested / released** separately.
- Never equate issue closure to real-world benchmark or player availability.
- No secrets or other people's account credentials in issues, logs, or import forms.
- Only show external community player counts when a project willingly publishes trustworthy aggregate figures.
- User-facing import flows require available source data, explicit consent, and a supported verification path.

See [ROADMAP](docs/ROADMAP.md) for the implementation sequence and [PROGRESS](docs/PROGRESS.md) for live status.
