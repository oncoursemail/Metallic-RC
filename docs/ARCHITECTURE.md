# Metallic-RC — Technical Reference Architecture (Proposed)

**Research baseline:** 2026-10-15  
**Decision status:** Candidate architecture, *not* evidence that services have been implemented.  
**Priorities:** independently hosted game, fast join times, protected infrastructure, highly usable creator tools, genuine desktop/VR parity, and a measured rather than promised performance advantage.

## 1. Important verified product baseline

- [Rec Room officially announced a shutdown effective June 1, 2026](https://blog.recroom.com/posts/schools-out-for-rec-room). Its legacy live service is **not** a suitable runtime dependency for new Metallic accounts, rooms or purchases.
- On [April 6, 2026](https://blog.recroom.com/posts/2026/4/6/exporting-your-creation-data), Rec Room documented Steam-PC-only export tools for underlying room, invention, and cloud data. They expressly said these do **not** export working room copies.
- On [April 21, 2026](https://blog.recroom.com/posts/2026/4/21/exporting-your-avatar-data), Rec Room documented 2D and 3D avatar export tools, with some items omitted unless their creators had enabled export. Metallic can design a converter for existing local exports **where compatible data is available**; restoring all live server-side account ownership is not automatic.

### Technology decision: compare prototypes, then pick

| Candidate | Strengths | Major risks/unknowns | Decision proof |
| --- | --- | --- | --- |
| **Unity + OpenXR** | Mature asset workflows, VR ecosystem, dedicated server build option, Unity Netcode and RRS-era asset compatibility concepts | Engine/package upgrades, platform builds, runtime memory and networking integration complexity | Ship measured small PCVR+Quest client and two-player dedicated room |
| **Godot 4.6 + OpenXR** | Open source, native Linux targets, flexible custom infrastructure, XR device work underway | Standalone mobile XR renderer and vendor-specific device work, fewer ready-made large social VR templates | Match same demo performance/comfort and AndroidXR export tests |
| **Unreal + OpenXR** | High rendering potential, tooling and built-in network replication | Client size, Quest/mobile VR performance complexity, dedicated server C++ expertise | Only consider if prototype meets low-end budgets |

Evidence: [Unity Netcode overview](https://docs.unity.com/en-us/multiplayer/netcode/netcode), [Unity dedicated-server docs](https://docs.unity.cn/6000.1/Documentation/Manual/multiplayer.html), [Godot XR March 2026 update](https://godotengine.org/article/godot-xr-update-mar-2026/), [Godot Android OpenXR guide](https://docs.godotengine.org/en/4.6/tutorials/xr/deploying_to_android.html).

**Recommended first experiment:** Unity client plus dedicated Unity-authoritative server (Netcode for GameObjects initially for a small player count), with a separately written Go/Rust backend for identity and rooms. It avoids building two different physics/simulation semantics prematurely. A standalone Rust simulation server is a later optimization candidate, not a free speed upgrade.

## 2. System context & trust boundaries

\`\`\`mermaid
flowchart LR
  U["Players: PC / VR / mobile"] --> CDN["HTTP CDN / WAF"]
  U --> RT["Protected real-time UDP ingress"]
  U --> VC["Voice SFU / WebRTC"]
  CDN --> API["Platform API: auth / profiles / rooms / marketplace"]
  API --> DB["PostgreSQL"]
  API --> Cache["Valkey cache / short-lived presence"]
  API --> OS["Object storage: avatars / rooms / releases"]
  API --> MQ["Event bus / outbox"]
  API --> MM["Room allocator / matchmaking"]
  MM --> GS["Authoritative game session fleet"]
  RT --> GS
  GS --> DB
  GS --> MET["OpenTelemetry / Prometheus / logs"]
  API --> MET
  ADMIN["Privileged admin UI"] --> API
  FED["Opt-in project registry"] --> API
  BOT["Discord bot / presence bridge"] --> API
\`\`\`

**Boundary A (untrusted clients):** all inventory/economy operations, room admission and moderation decisions are verified server-side. The client can predict movement but cannot mint currency or select protected server state.

**Boundary B (public HTTP edge):** authenticates user sessions, enforces token and request limits, provides API for web and launcher. Separate app/admin/public scopes and CSRF protections.

**Boundary C (real-time edge):** clients receive scoped, signed, short-lived join tickets; session servers validate tickets and expiry. UDP ingress and WebRTC media traffic need their own protection model, not ordinary web WAF alone.

**Boundary D (creator content):** packages are quarantined, scanned, size/rate limited, signed by publishing pipeline, approved as appropriate, and loaded in resource-restricted runtimes.

**Boundary E (federation):** third-party project metadata is untrusted unless cryptographically verified and explicitly allowlisted.

## 3. Practical implementation structure

Suggested monorepo after engine decision:

\`\`\`text
Metallic/
  README.md
  docs/
    ROADMAP.md  PROGRESS.md  ARCHITECTURE.md  RESEARCH.md
    adr/  api/  operations/  benchmarks/  release-evidence/
  .github/
    workflows/
    ISSUE_TEMPLATE/
  client/                  # engine-specific source and XR setup
  launcher/                # signed desktop launcher and repair updater
  platform/
    api/                   # HTTP public API and app credentials
    identity/              # authentication/account/privacy
    rooms/                 # directory, matchmaking and presence
    creator/               # publications, review and asset registry
    economy/               # ledger and purchase orchestration
    moderation/            # reports, appeals and audit trail
    federation/            # opt-in partner manifests + status APIs
  game/
    server/                # authoritative sessions
    netcode/               # movement replication & protocol schemas
    content/               # originals / manifests / prefabs
    circuits/              # typed visual graph IR and runtime
  web/
    site/                  # public site, room catalogue, downloads
    creator/               # creator dashboard
    admin/                 # privileged control panel
  ops/
    local/                 # docker compose/dev environment
    deploy/                # server deployment manifests
    dashboards/            # actual operational metrics
    loadtest/              # synthetic game clients
  scripts/
  tests/
\`\`\`

Do **not** split this into 30 microservices on day one. Start as a modular monolith for business APIs, a separate authoritative session process, and independent voice service. Split hot paths only when measured contention warrants.

### Candidate stack

| Layer | First choice to evaluate | Backup candidate | Evidence required |
| --- | --- | --- | --- |
| Client game/VR | Unity + OpenXR | Godot + OpenXR | 72/90 Hz on selected VR devices |
| Dedicated game sessions | Unity dedicated server for MVP | Custom Rust authoritative sim | 2/16/32 player tests, correct physics and CPU baseline |
| API backend | Go or Rust modular server | C#/.NET API | Dev skill level, maintainability, response p95 |
| Database | PostgreSQL | Managed Postgres | Transactional integrity + restore |
| Cache / presence TTL | Valkey | Redis-compatible provider | TTL correctness, failure recovery |
| Messages | PostgreSQL outbox first | NATS JetStream when needed | No event loss under restart |
| Search | Postgres full-text initially | Meilisearch/Typesense | Ranking, moderation-aware filters, index freshness |
| Voice | Self-hosted LiveKit SFU pilot | Alternative tested SFU | Opus quality, regional routing, abuse controls |
| Assets | S3-compatible object store | Cloudflare R2 for served media | Egress, consistency, signed URLs, backups |
| Telemetry | OpenTelemetry + Prometheus/Grafana | Managed hosting | Measured CPU cost + useful dashboards |
| Site/admin | TypeScript web frontend | Equivalent web stack | Accessibility, permission tests, metrics |
| Launcher | Tauri/Rust candidate | Native launcher | Signature verification, interrupted-update recovery |

LiveKit is an [open-source self-hostable SFU](https://github.com/livekit/livekit); its [SFU design](https://docs.livekit.io/reference/internals/livekit-sfu/) routes compressed tracks and is separate from the authoritative world-state server.

## 4. Control plane and game plane APIs

Version all public endpoints under \`/v1\`; consider Protobuf for game-server/client payloads. These are proposed **contract examples**, not functioning endpoints.

\`\`\`http
POST /v1/auth/register
POST /v1/auth/login
POST /v1/auth/session/refresh
POST /v1/auth/logout
GET  /v1/me
PATCH /v1/me/privacy
GET  /v1/rooms?tags=coop&sort=relevance&cursor=...
POST /v1/rooms/{room_id}/join
POST /v1/rooms/{room_id}/leave
GET  /v1/sessions/{session_id}/status
GET  /v1/creator/packages/{package_id}/versions
POST /v1/creator/packages/upload-session
POST /v1/creator/rooms/{room_id}/publish
GET  /v1/marketplace/items?type=room
POST /v1/marketplace/purchases
GET  /v1/federation/projects
GET  /v1/federation/projects/{project_id}/status
POST /v1/reports
GET  /v1/admin/audit-events
\`\`\`

**Join flow**
1. Launcher authenticates user with scoped short-lived credentials.
2. Client requests \`POST /v1/rooms/{id}/join\` with compatible build/protocol version and local device capability.
3. Matchmaker selects region and session with capacity. API returns short-lived opaque join ticket, server endpoint, protocol version and voice capability.
4. Game server validates signature, expiry, nonce, room, user, and consumed-ticket state; server sends baseline world snapshot.
5. Client predicts locally; server reconciles input, timestamps and state. Presence TTL renews from server heartbeats.
6. Disconnect/reconnect restores session state where feasible with ticket replay protection.

**Illustrative response**
\`\`\`json
{
  "sessionId": "session_uuid",
  "roomId": "room_uuid",
  "region": "us-central",
  "protocolVersion": 1,
  "joinTicket": "short_lived_signed_opaque_token",
  "expiresInSeconds": 45,
  "endpoints": {"game": "game.example.org:26000"},
  "voice": {"available": true, "enabledByDefault": false}
}
\`\`\`

**Database boundaries (conceptual)**
- \`users\`, \`user_auth_methods\`, \`sessions\`, \`user_privacy\`
- \`profiles\`, \`avatars\`, \`inventory_entitlements\`, \`asset_revisions\`
- \`rooms\`, \`room_versions\`, \`room_acl\`, \`room_sessions\`, \`presence\`
- \`reports\`, \`moderation_cases\`, \`sanctions\`, \`audit_events\`
- \`ledger_accounts\`, \`ledger_entries\`, \`purchases\`, \`refunds\`
- \`federation_partners\`, \`signed_manifests\`, \`partner_status_snapshots\`

Enforce unique active entitlements and payment idempotency at the database level. Avoid deriving spendable balance from unverified client messages or a cache-only value.

## 5. Realtime replication & performance engineering

A fixed-step room tick with event queue isolates simulation from render/VR refresh. Recommended initial experiments: 20–30 simulation ticks/sec; head/hands updates can use an interpolated replication stream; adjust only after side-by-side jitter, CPU and latency results.

**Bandwidth approximation** (before framing overhead, retransmission and voice):  
\`total outbound Mbps = concurrent_users * average_server_to_client_kbps / 1000\`

Examples under a *hypothetical* 96 kbps outbound world-state stream per user:

| Concurrent players | Outbound world-state only | Approx outbound/month at 24/7 utilization |
| ---: | ---: | ---: |
| 100 | 9.6 Mbps | ~3.1 TB |
| 1,000 | 96 Mbps | ~31.1 TB |
| 10,000 | 960 Mbps | ~311 TB |

These are sizing scenarios, **not proven requirements**. Real results depend on room density, frequency, transform compression, voice and playtime duty cycle. Traffic is asymmetric and peak sizing matters.

**Optimizations to benchmark:**
- Interest grid / area-of-interest: only relevant nearby actors update at high frequency.
- Reliable events (inventory, quest completion) versus high-frequency unreliable state packets.
- Quantized transforms, delta-compressed snapshots, pooled buffers, no hot-path allocations.
- Progressive asset streaming, texture atlases, occlusion, level-of-detail and graphics presets.
- Quest server logic should cap per-user execution time and circuit graph operations.
- Per-region latency telemetry and server CPU/memory/queue depth alarms.

**Performance quality gates:** publish hardware model, driver/runtime, headset refresh, patch build, scene complexity, percentiles (p50/p95/p99), and profiling traces. Report 72/90 Hz target as a *goal* until demonstrated.

## 6. Creator tools & Circuits V3 implementation

Build a **typed graph compiler**, not a dynamic arbitrary script executor inside rooms.

\`\`\`text
Creator editing UI
  -> Typed node graph {graphId, version, nodes, pins, connections}
  -> Validator [types, allowed capabilities, ownership, cycles, budgets]
  -> Intermediate representation (IR)
  -> Versioned bytecode / interpreter
  -> Simulation runtime [tick quota, memory quota, event quota]
  -> Auditable effects [spawn, animate, sound, score, save]
\`\`\`

**First node types:** OnRoomStart, OnPlayerEnterZone, OnInteract, OnTick (limited), And/Or/Not, Float/Int Arithmetic, Compare, If, Timer, SetVariable, GetVariable, TriggerAnimation, TeleportAuthorizedPlayer, ScoreUpdate.

**Constraints:** node count, event fan-out, max depth per tick, server trust controls, deterministic timers, persistent-store rate limits and variable migration. Abort runaway circuits without crashing whole sessions. Creator debugger should show the node that exceeded its budget with useful visual trace.

**Creator UX acceptance:** placement gizmos and snapping, precise inspector, hover highlight, undo history, clear selected-state feedback, input parity for mouse/controller, save recoverability, and profiling guidance inside the editor. Avoid shipping CV3 before Maker Pen persistence is stable.

## 7. Signed launcher & federation design

Use a versioned manifest with file hashes, signature and expiry; protect against rollback. A [TUF-style update trust model](https://theupdateframework.io/) provides a stronger reference than checking TLS alone.

Example conceptual launcher manifest:

\`\`\`json
{
  "schema": 1,
  "projectId": "metallic",
  "displayName": "Metallic-RC",
  "channel": "alpha",
  "build": "0.1.0-alpha.1",
  "platforms": ["windows-x64"],
  "artifact": {"url": "https://downloads.example.org/build.zip", "sha256": "<real_digest>"},
  "minLauncherVersion": "0.1.0",
  "expiresAt": "2026-12-01T00:00:00Z",
  "signature": "<signature_over_canonical_manifest>"
}
\`\`\`

Never ship those placeholder values to production. Store partner registry public keys outside downloadable partner-controlled metadata, validate hostnames, prevent manifest redirect to untrusted binaries, and sandbox each partner's install directory.

**Federation requires opt-in partners.** Standard:
- \`/.well-known/metallic-federation.json\`: project capabilities and signing keys.
- \`/v1/status\`: signed \`totalOnline\`, \`source\`, \`observedAt\`, \`expiresAt\`.
- Optional per-room public counts, compatible signed launcher manifests and creator directory.
- UI distinguishes **verified aggregate**, **partner-reported**, **cached/stale**, **unknown**.

Without partnership or published APIs, counts are unknown; do not present scraped or fabricated figures as authoritative.

## 8. Account and content migration architecture

Given the official June 2026 shutdown, **new live logins to the original service are not an available verification path**. Design an importer that can accept material the player already exported or can independently verify. Proposed pipeline:

\`\`\`text
Local export upload -> type sniffing -> schema version probe -> antivirus/safe unzip
 -> dry-run mapping -> list supported/unsupported fields -> consent screen
 -> claim evidence/manual review when needed -> transform -> player preview
 -> isolated staging record -> commit import -> audit receipt -> deletion policy
\`\`\`

Separate **username claim**, **avatar model**, **room data**, **inventions**, **cloud state**, **friends**, and **purchased entitlements** as different data categories. User-owned local files are not proof that all original backend entitlements, friends and purchases can be reconstituted.

## 9. Discord, admin, trust and creator economy

- Use [Discord Social SDK Rich Presence](https://discord.com/developers/docs/rich-presence/overview) where supported; opt-in default-off activity sharing, mask private rooms, immediate user toggle, privacy-aware join invites.
- Independent Discord bot for announcements, bot commands, website moderation notices and user-linked roles; its privileged bot token stays only on server.
- Separate operations console privileges: superadmin, incident responder, moderator, creator reviewer, support, read-only analytics. Every sensitive action writes an audit trail.
- Marketplace backed by durable double-entry transaction records and finite entitlements; audit synthetic purchases/refunds before involving payment services.
- For voice safety, offer mute/block/report, local user consent and reports with constrained retention and human-review escalation.

## 10. Self-owned infrastructure and DDoS defense

**Do not conflate web protection with UDP game protection.** [Cloudflare Spectrum](https://developers.cloudflare.com/spectrum/) documentation states custom TCP/UDP services require paid Enterprise add-on arrangements; a free proxied DNS/web plan does not shield arbitrary Unity game UDP ports.

**Defensive reference topology**
- Web, status, API on a protected HTTPS edge; restrict origin ingress to intended traffic.
- Dedicated and separately protected game-session UDP ingress or provider-level game hosting/scrubbing; upstream filtering is needed before a saturating attack reaches a small ISP link.
- Voice SFU with separate protection and bandwidth capacity planning.
- Isolate database/admin/control networks from public session ingress via firewall + private overlay.
- Distribute releases through CDN/object storage with immutable version paths.
- Back up identity/economy data offsite; regularly test restore and emergency shutoff/safe-mode.
- Alert on connection velocity, handshake failures, bytes/sec, SYN/UDP saturation, CPU/packet drops, auth anomalies, media packet loss and bad deployments.

Home servers are fine for small prototypes; they cannot guarantee uninterrupted public service during upstream saturation. Seek colocation/hosting with an actual protected UDP offering before public-scale beta.

## 11. First three implementation experiments

1. **Engine decision benchmark:** same small original social scene, two avatars, head/hand tracking and authored interaction in Unity/OpenXR and Godot/OpenXR; publish frametime capture and device matrix.
2. **Join protocol contract test:** minimal API, JWT/opaque join ticket, single room allocator, two Unity/Godot clients and server replay/failure tests.
3. **Creator data round trip:** editor object -> serialized snapshot -> Postgres metadata + object blob -> authoritative room loads revision -> screenshot and deterministic validation.

Stop expanding scope until these pass; they de-risk the launcher, multiplayer and maker tools simultaneously.

## 12. Source pointers

- Rec Room closure and exports: https://blog.recroom.com/posts/schools-out-for-rec-room ; https://blog.recroom.com/posts/2026/4/6/exporting-your-creation-data ; https://blog.recroom.com/posts/2026/4/21/exporting-your-avatar-data
- Unity multiplayer: https://docs.unity.com/en-us/multiplayer/netcode/netcode
- Godot XR 2026: https://godotengine.org/article/godot-xr-update-mar-2026/
- Godot Android/OpenXR: https://docs.godotengine.org/en/4.6/tutorials/xr/deploying_to_android.html
- LiveKit server and SFU: https://github.com/livekit/livekit ; https://docs.livekit.io/reference/internals/livekit-sfu/
- Discord Rich Presence: https://discord.com/developers/docs/rich-presence/overview
- Cloudflare Spectrum: https://developers.cloudflare.com/spectrum/
- The Update Framework: https://theupdateframework.io/
- [Continually updated research catalogue](RESEARCH.md)
