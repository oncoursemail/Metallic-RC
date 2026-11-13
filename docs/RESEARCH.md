# Metallic-RC — Research Register and Experiment Backlog

**Research cut-off:** 2026-10-15 · **Scope:** product parity, competitor implementations, public/open-source tools, security, VR, performance and product quality. Legal analysis is not the focus of this document.  
**Confidence scale:** **Verified source** = official announcement/maintained project README, **Project claim** = untested author statement, **Unknown** = no verified evidence yet, **Proposal** = original Metallic design.

## 1. Baseline: Rec Room's final 2026 state

| Finding | Evidence | Confidence | Implication for Metallic |
| --- | --- | --- | --- |
| Official Rec Room service scheduled to shut down June 1, 2026 | [Rec Room announcement, Mar 30 2026](https://blog.recroom.com/posts/schools-out-for-rec-room) | Verified source | Build fully independent identity, matchmaking and rooms |
| Rec Room's March 30 ship notes describe loss of login and Rec Room Studio services at shutdown | [Official ship notes](https://recroom.com/ship-notes) | Verified source | Studio replacement cannot rely on original online APIs |
| Official creation data export tools published for updated Steam PC client on Apr 6 | [Creation exports](https://blog.recroom.com/posts/2026/4/6/exporting-your-creation-data) | Verified source | Design a *local export file* converter and partial room recreation workflow |
| Creation exports do not contain directly playable, working rooms | [Creation exports](https://blog.recroom.com/posts/2026/4/6/exporting-your-creation-data) | Verified source | Build own scene/asset format, interpreter and migration diagnostics |
| Official avatar 2D/3D export offered Apr 21; some assets absent depending on creator consent | [Avatar exports](https://blog.recroom.com/posts/2026/4/21/exporting-your-avatar-data) | Verified source | Build preview/partial mapping and substitute missing components transparently |
| In the March 11 2026 update, CV2 multiplayer synced variables and state machines received fixes; Studio Functions gained properties/local vars/utilities | [Mar 11 2026 ship notes](https://ag.recroom.network/ship-notes/2026/3/11/-rec-room-update-the-couple-small-things-update) | Verified source | CV3 must have deterministic synchronized state, rollback/debug and explicit graph upgrade tests |
| Historical Rec Room Studio used Unity editor, asset importing, collaborative Maker Pen edits and programmable circuits | [Rec Room Studio](https://ag.recroom.network/studio) | Verified historical description | Unity import interoperability can be investigated; avoid claiming original services remain live |

**Unverified:** the precise final client binary/build ID, all features of every 2026 version, usable credentials/entitlements after shutdown, and current status of every fan revival. Record exact source commit/release for any claim before turning it into a compatibility promise.

## 2. Preservation/revival ecosystem evidence (as of this register)

| Project | Public evidence found | What can be said | Unknown / next verification |
| --- | --- | --- | --- |
| **RestoRoom** | [Repository](https://github.com/RestoRoom/RestoRoom), [project site](https://restoroom.poime.net/) | Repo describes an early community server reimplementation. Author's README characterizes it as **not playable yet**, with authentication/server work still planned. | Check latest commit, tagged releases, actual integration tests and public gameplay before asserting support or capability |
| **neorec** | [Repository](https://github.com/therock64/neorec) | Modern 2026 protocol notes/reference server; README explicitly says it is **not a working private server**. | Validate protocol claims against independently reproducible tests, version, and maintained code |
| **MeowNet launcher (one public repo)** | [Archived launcher repo](https://github.com/hollyntt/MeowNet-Launcher) | This *specific* launcher was archived June 12, 2026; README says it lacked autoupdates, installing games and embedded website at that time. | Do not infer current MeowNet service status or newer client capabilities from this one archived launcher |
| **Radium** | Mentioned in a community [neorec README](https://github.com/therock64/neorec) | Evidence only that a developer reported historical-client revivals exist. | Find verified project-owned documentation, maintained repositories, builds and compatibility manifests |
| **Phoenix Studios, Vanilla, Campus, Stella, other revivals** | Names supplied as comparison targets | **Not verified in this register**. | Confirm canonical website/repo/operator; version, active users, launcher, security posture, assets, supported platforms and source availability. Do not fabricate comparative scores |

**Research principle:** treat GitHub README features as claims until an issue, release, test artifact, or verified demonstration proves execution.

## 3. Benchmarks for 'better than other revivals'

Rather than self-declared 'best', compare on a reproducible feature scorecard:

| Quality dimension | Evidence metric | Pass target for beta (proposed) |
| --- | --- | --- |
| Desktop stability | Crash-free sessions (measured) | Set SLO after alpha; publish number |
| VR smoothness | Frame-time p95, device matrix, real scene | 72/90 Hz based on verified headset profile |
| Room join | p50/p95 time measured across regions | p95 <5 seconds after assets cached |
| Recovery | Reconnect, restart, state persistence | Demonstrated test under packet loss and process restart |
| Creator reliability | Save/load/undo/revision round trips | Zero lost transactions in integration tests |
| CV3 correctness | Sandboxed scripts, multiplayer state determinism | All conformance fixtures pass |
| Abuse controls | Report -> reviewer -> resolution and appeals | Audit trail on every staff action |
| Marketplace | Ledger reconciliation, idempotent purchase | Zero unexplained balance drift in sandbox testing |
| Federation | Signed third-party status and opt-in manifest | Correct revocation/staleness behavior |
| Origin defense | Layered provider-backed game UDP and HTTP protection | Runbook tabletop + failover rehearsal |

Public scoreboard **must leave cells unknown** where no evidence exists.

## 4. Open-source and vendor study shortlist

The list is a reference for experiments, **not installed dependencies**.

| Area | Options to inspect | Research action |
| --- | --- | --- |
| XR clients | [Unity multiplayer](https://docs.unity.com/en-us/multiplayer/netcode/netcode), [Godot XR](https://godotengine.org/article/godot-xr-update-mar-2026/) | Compare one identical VR scene on PC + target mobile headset |
| Voice/SFU | [LiveKit](https://github.com/livekit/livekit) | Test self-hosted Opus media under loss + moderation workflow |
| API/database | [PostgreSQL](https://www.postgresql.org/), [Valkey](https://valkey.io/), [NATS](https://nats.io/) | Test atomic purchases, presence TTL, reconnect, reliable events |
| Object storage | [Cloudflare R2](https://developers.cloudflare.com/r2/), [MinIO](https://github.com/minio/minio) | Verify license, current maintenance/distribution, pricing, S3 compatibility and regional egress |
| Identity | [Keycloak](https://www.keycloak.org/), [Authentik](https://goauthentik.io/) | Compare OIDC, passkeys, MFA and admin separation |
| Search | [Meilisearch](https://www.meilisearch.com/), [Typesense](https://typesense.org/), PostgreSQL FTS | Benchmark room tags/filtering, data freshness, moderation |
| Monitoring | [OpenTelemetry](https://opentelemetry.io/), [Prometheus](https://prometheus.io/), [Grafana](https://grafana.com/), [Loki](https://grafana.com/oss/loki/) | Trace login-to-room-join and target alert cardinality |
| Authenticated website | [Cloudflare Workers](https://developers.cloudflare.com/workers/), [Firebase](https://firebase.google.com/), [Supabase](https://supabase.com/) | Record currently published quotas, data costs and vendor dependence |
| Scripting | [WebAssembly](https://webassembly.org/), typed custom graph IR | Evaluate sandboxing, deterministic quotas, graph editor performance |
| Desktop launcher | [Tauri](https://tauri.app/), [The Update Framework](https://theupdateframework.io/) | Validate signed manifests, rollback, download resume and offline repair |
| Federation | [OpenAPI](https://www.openapis.org/), JSON signatures, mTLS for admin partners | Two dummy projects with allowlists, timestamps and revoked keys |
| Discord | [Discord Social SDK/Rich Presence](https://discord.com/developers/docs/rich-presence/overview) | Test desktop join invites and privacy-off toggles |
| Game-host DDoS | [Cloudflare Spectrum](https://developers.cloudflare.com/spectrum/), provider UDP scrubbing, network segmentation | Confirm real UDP support, upstream SLA and cost (web-only free WAF is not enough) |

**Pricing rule:** do not assume 'free tier' includes unlimited game traffic, voice media, private CI minutes or full UDP DDoS protection. Obtain fresh official quotes and model peak-to-average usage before deciding.

## 5. Research experiments — work in measurable order

### R-01: Determine the feasible 2026 client compatibility path
- Compare original-game vertical slice, protocol-compatible independently distributed client, and opt-in partner integration.
- Artifact: data flow diagrams, specific client/runtime build evidence, unknown field list, measured prototype.
- Do not assert all 2026 features can be reproduced without confirmed source access and tested endpoints.

### R-02: Analyze export samples (obtained from legitimate user-provided files)
- Identify 2D/3D avatar export schemas, geometry, rig mapping, room/invention data records and unknown asset references.
- Artifact: sample fixtures with sensitive data removed, schema parser tests and a non-destructive import preview.
- Completion = parse and render supported sample fields and clearly flag everything not supported.

### R-03: Benchmark PCVR plus standalone headset
- Compare OpenXR activation, controllers, hand/head sync, asset bundles, graphics quality tiers, CPU/GPU 95th percentile frame time.
- Artifact: reproducible build and profiling capture.

### R-04: Measure end-to-end room lifecycle
- Test account -> lobby -> join ticket -> session -> voice -> disconnect -> reconnect and mismatched protocol versions.
- Artifact: OpenAPI/protocol fixtures, load report and test dashboards.

### R-05: Prototype CV3 as a typed deterministic graph
- Build node types, graph validation, interpreter, safe per-tick quotas and authoritative execution.
- Artifact: event door + timer + multiplayer score graph, property-based tests, budget violation traces.

### R-06: Protect self-hosted infrastructure
- Compare house ISP, colo and leased provider designs; map ingress to HTTP/CDN, game UDP and SFU media.
- Artifact: diagram, supplier quote assumptions, firewall rules and tabletop response plan.

### R-07: Test creation studio and first-rate UI quality
- Prototype a truly native-feeling editor with consistent typography, high quality input/animation, responsive layouts, accessibility and efficient scene operations.
- Artifact: Figma reference system, annotated screen captures and usability/playtest observations.

### R-08: Community and revival compatibility audit
- For each candidate, find official public links, public downloadable artifacts and feature tests; update only verified scorecard fields.
- Artifact: dated CSV/Markdown competitor matrix with link and evidence classification.

### R-09: Understand server bandwidth economics
- Measure outbound world state, voice SFU, CDN downloads, storage, backups per concurrent user at 10/100/1000 active.
- Artifact: spreadsheet/CSV with assumptions, traffic confidence and price scenario.

### R-10: Harden launcher update supply chain
- Threat-model hijacked mirror, invalid signature, downgrade, interrupted install, revoked partner manifest and compromised build machine.
- Artifact: test suite that proves every bad update is rejected.

## 6. Unique feature research backlog

These are **ideas**, not finished specs or shipped features:
- **Metallic Studio Lab:** multiplayer authoring workspace with author-controlled collaborative review and local preview.
- **CV3 debugger:** live variable inspector, frame-step replay, graph-level performance hotspots, seeded test scenes.
- **Room health assistant:** translate real telemetry into actionable creator budget advice (draw calls, network events, script quotas), with confidence indicators.
- **Federation trust scoreboard:** present last-seen signed online status and reliability, not fabricated concurrent players.
- **Legacy import assistant:** clear preview of which local export assets can be mapped, repair options, missing dependencies and staged upload.
- **Creator staging channels:** private beta room with rollback/release notes, then public audience.
- **Original quest creator framework:** prefab quest rules, checklists, analytics, co-op objectives and reusable original enemy AI.
- **Player-controlled social privacy:** per-room invitation controls, hidden sessions, show/hide activity, cross-platform transparency.
- **Network debugging in VR:** lightweight real-time netgraph visible only to tester/admin, with privacy and performance budgets.
- **Moderation evidence viewer:** privacy-safe case records and escalation trails rather than irreversible opaque action.
- **Server operator dashboard:** regional room heatmap, active sessions, p95 latency, CPU/RAM/capacity, incident notes.
- **Deterministic replay harness:** record synthetic game inputs, replay known bugs and compare server state hashes in CI.

See [FEATURE_CATALOG.md](FEATURE_CATALOG.md) for expanded prioritized capability candidates once created.

## 7. Unanswered questions that block safe implementation

1. Which engine and exported project files (if any) are actually present, and which official 2026 build/asset formats are available for testing?
2. What is the true scope of the claimed 'working account import' (local files vs verified server fields vs original entitlements)?
3. What are the chosen first two VR headsets and minimum PC/mobile devices?
4. What upstream DDoS mitigation is included by the physical server host/ISP?
5. Which third-party revivals have documented opt-in API contracts and permission to use their status data?
6. Is the project designed to be a fully independent game client, a compatible client where feasible, or multiple distributions? Track each as separate workstream to avoid ambiguity.
7. What precisely counts as 'all 2026 features'? Create a source-linked versioned list, then coverage tests.
8. Which pieces should be released as open source vs kept private (server secrets never belong in repo)?

## 8. Update policy

- Date-stamp every source checked; preserve permalinks and release/build IDs where available.
- Separate **source statement**, **measured experiment**, **inference**, **proposal** and **unknown**.
- Keep the actual GitHub issue tracker and [auto-generated progress](PROGRESS.md) as the source of completed work; no marketing claims without proof.
- Research questions that become actionable work should become \`[P0]\`–\`[P6]\` issues with acceptance tests.
