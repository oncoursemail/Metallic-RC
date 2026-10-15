# Metallic Studios

> **Project status: Foundation planning — not yet a playable game.**  
> Independent social multiplayer, VR, and creator platform. The repository starts with a technical roadmap and evidence-based progress tracker; completed features must be proven by merged changes, tests, and working builds.

[![Roadmap & progress](https://img.shields.io/badge/Roadmap-Live%20progress-24292f?logo=github)](docs/PROGRESS.md)
[![Project plan](https://img.shields.io/badge/Plan-Engineering%20roadmap-323f53)](docs/ROADMAP.md)

## What we are building

Metallic Studios is intended to be an independent, high-performance social game platform with its own client, dedicated servers, launcher, identity service, creator tools, user-generated rooms, visual scripting, and optional integrations with cooperating community projects.

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
- **[Research backlog](docs/RESEARCH.md)** — sources, experiments, unknowns, and competitor review.
- **[Tracked work items](../../issues)** — one issue per verifiable deliverable.
- **[Pull requests](../../pulls)** — proposed changes and review history.

The `Update project progress` GitHub Actions workflow refreshes the Markdown dashboard after relevant issue/PR changes, on demand, and daily. Issues use `[P0]` to `[P6]` title prefixes; the updater counts open/closed issues in each stage and refuses to fabricate implementation progress. An issue should close only when its linked acceptance tests have passed.

## Initial milestone

**First playable vertical slice:** a signed desktop test build connects to an independently hosted authoritative server, joins a room with two players, replicates movement, shows a basic avatar, sends optional positional voice, and survives reconnects. We deliberately defer marketplace, account import, and federation until the foundation is safe and testable.

## Platform and tech candidates

- **Client:** Unity + OpenXR **or** a tested Godot/OpenXR path; decision recorded after performance and licensing comparison.
- **Backend:** Rust or Go authoritative game sessions; a modular HTTP API; PostgreSQL; Valkey; object storage; OpenTelemetry.
- **Website + launcher:** TypeScript web console, Tauri/Rust desktop launcher candidate.
- **Creator runtime:** versioned graph schema, deterministic/server-authoritative execution for trusted game logic.
- **Infrastructure:** self-owned machines for dev/test; provider-protected networking for public UDP sessions. Cloudflare's regular HTTP proxy is not a complete UDP game-server DDoS solution.

Technology choices remain proposals until measured prototypes and an architecture decision record (ADR) are committed.

## Principles

- Report **planned / in progress / blocked / tested / released** separately.
- Never equate issue closure to real-world benchmark or player availability.
- No secrets or other people's account credentials in issues, logs, or import forms.
- Only show external community player counts when a project willingly publishes trustworthy aggregate figures.
- User-facing import flows require available source data, explicit consent, and a supported verification path.

See [ROADMAP](docs/ROADMAP.md) for the implementation sequence and [PROGRESS](docs/PROGRESS.md) for live status.
