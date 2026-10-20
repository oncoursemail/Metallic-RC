# Metallic-RC — High-Performance Global Multiplayer, Own Infrastructure, Anti-Cheat & Cross-Platform Blueprint

**Research date:** 2026-10-18. **Status:** Research and engineering specification; nothing here implies a live fleet, playable Linux client, deployed anti-cheat, a performance advantage, or any production benchmark. **Fits:** [Architecture](ARCHITECTURE.md), [Roadmap](ROADMAP.md), [Quality & stress tests](QUALITY_AND_STRESS_TESTING.md), issues [#10](https://github.com/oncoursemail/Metallic-RC/issues/10), [#14](https://github.com/oncoursemail/Metallic-RC/issues/14), [#28](https://github.com/oncoursemail/Metallic-RC/issues/28), [#39](https://github.com/oncoursemail/Metallic-RC/issues/39), [#50](https://github.com/oncoursemail/Metallic-RC/issues/50).

## 1. The actual competitive objective

Build a *more responsive, accessible, secure, creator-friendly* independent social platform than the historical Rec Room experience. Rec Room [announced closing on June 1, 2026](https://blog.recroom.com/posts/schools-out-for-rec-room), explicitly citing costs outweighing revenue. Hence **cost per stable connected player** is as important as peak throughput.

Four different capacity claims must never be conflated:
1. **Accounts:** registered user count, not concurrency.
2. **CCU:** thousands logged into the *whole platform* across rooms and regions.
3. **Players in one room:** more demanding because visible actor interactions and broadcasting increase superlinearly without interest management.
4. **Players in one seamless shared world:** dramatically harder; requires partitioning, inter-cell handoff, relevance culling, state synchronization, and large-world game design changes.

**Proposed goals, not verified achievements:** 10,000 concurrent regional players as an architecture planning scenario; validate 2 → 16 → 32 → 64 per room empirically; 100,000 platform CCU only as a later economic/technical modeling exercise; do not promise a 1,000-avatar single room or no-lag globe.

## 2. Architecture: own the game, not every commodity primitive

Core decisions:
- **Own authoritative game logic, creator runtime, replication format, accounts, inventory/economy trust rules, crossplay contract, anti-cheat evidence, experience orchestration and player support.**
- Use independently managed deployment of proven infrastructure: PostgreSQL (transactions), Valkey (ephemeral presence/cache), S3-compatible blob store/CDN, OpenTelemetry + Prometheus/Grafana, Kubernetes + [Agones](https://agones.dev/site/docs/overview/) only after a small fleet has been proven, self-hosted [LiveKit](https://docs.livekit.io/transport/self-hosting/) or equivalent for media.
- Keep public HTTP/API, UDP gameplay and WebRTC voice **separate**; ordinary CDN/WAF does not protect custom game UDP automatically. Use providers with actual UDP DDoS capabilities, test origin lock-down.
- Start modular monolith for account/social/creator APIs, separated real-time game server process and separately scaled voice SFU. Do not begin with dozens of microservices.
- Independently hosted means Metallic manages credentials, deployments, source, assets, monitoring and game availability; leased cloud/bare-metal can still satisfy this. Owning worldwide physical network hardware is not a prerequisite and is often economically inferior early.

### Proposed systems diagram

    [Windows / Linux / SteamOS / Quest / PCVR / Android / iOS]
          | HTTPS                   | secured real-time packets          | voice
          v                         v                                    v
    [WAF + API edge]          [UDP protection/ingress]             [regional voice SFU]
          |                         |
    [Metallic Control Plane]   [Metallic Session Fleet]
          |                    [authoritative room processes]
    [Auth + profiles]                |
    [Room registry] <--- [allocator: region / protocol / build / capacity]
    [creator catalogue]             |
    [moderation/appeals]      [zone/interest management + physics + Circuits]
          |                         |
    [Postgres + outbox]        [validated event emission]
    [Valkey + presence TTL]          |
    [Object store/CDN]         [event consumer, idempotency]
          \__________________ [Telemetry / alerting / backup / on-call]

**Hard boundaries:** public identity token ≠ game join ticket; join ticket ≠ staff entitlement; client input ≠ authoritative state; region-level presence ≠ monetized purchases; uploaded content ≠ trusted code; media SFU ≠ gameplay simulation.

### Named Metallic-owned application components (design names only)
| System | Function | Build/buy boundary |
| --- | --- | --- |
| **Metallic Atlas** | player-facing region and room selection; latency and residency-aware allocation; queue and reservations | Metallic policy on top of Agones/initial process supervisor |
| **Metallic Nexus** | tick, interest management, snapshots, input reconciliation, scene budgets and game rules | proprietary core |
| **Metallic Sentinel** | server validation, evidence pipeline, risk signals, moderation queue, appeals and anti-cheat telemetry | proprietary policy; optional evaluated vendor integration later |
| **Metallic Pulse** | tick/client telemetry, capacity decisions, operator SLOs, incident dashboard | proprietary dashboards built on OTel, Prometheus/Grafana |
| **Metallic Forge** | reproducible signed client/server builds, content compatibility and safe progressive rollout | proprietary policies atop CI, registry, signature tooling |
| **Metallic Relay** | signed UDP entry endpoints / NAT connectivity, traffic accounting, abusive-traffic thresholds | prioritize battle-tested transport and edge vendor, never homemade cryptography |

## 3. Authoritative multiplayer: making rooms feel fast

**First network experiment:** Unity Dedicated Server (Linux) plus [Netcode for GameObjects](https://docs.unity.com/en-us/multiplayer/netcode/netcode) for simple 2–16 player proof; compare [Netcode for Entities](https://docs.unity.com/en-us/multiplayer/netcode/netcode) when higher density, prediction or DOTS scene warrants it. Evaluate Godot + headless export independently. A custom Rust simulation is a *later* measured optimization, not assumed to be automatically faster or physics-identical.

- Fixed-step gameplay tick *separate* from headset render frequency. Test 20, 30 and 60 Hz under identical workload, choose by worst-case tick time, network bytes/player, physics feel and client correction.
- **Inputs, not positions:** clients send numbered/sanitized move, jump, grab, emote, tool, and VR head/hand samples, rate-limited and scoped; server simulates movement, ownership, hit and scoring.
- Client prediction/authoritative reconciliation for own motion, interpolation of remote avatars, bounded lag compensation in competitive modes, packet pacing and capped extrapolation under loss.
- Replicate relevance-ranked **delta snapshots** at adaptive rates and acknowledge baselines; object creation, critical inventory and game state transitions use reliable channels and explicit idempotency.
- Spatial cell/grid/BVH area-of-interest (AOI): near actors 20–30 Hz target to test, mid-distance lower frequency, distant actors low-cost silhouettes or omitted. Per-user LOD is distinct from graphics LOD. Queue fairness ensures rarely-updated entities eventually refresh. Never rely on client requesting invisibility as security.
- Quantize poses/quaternions, compact identifiers/bit packing, pooled buffers, bounded reliable backlog, entity lifetime IDs, content manifest caches; benchmark each against visual correctness and CPU.
- VR head/hand physics has special constraints: disallow impossible server-effects/teleport bypasses while still allowing accessibility movement, tracking loss, seated users and room-scale repositioning; do not blanket-ban people with unusual mobility.
- Creators: untrusted Circuits node graphs compiled to validated IR with CPU/fuel/memory/event-rate quotas; never run arbitrary uploaded native code in authority process; server owns published object limits and ownership.
- Large events: prefer **instanced viewing / regional crowd lobbies** initially. Seamless 1000+ shared areas require sharded simulation (cell ownership, cross-border entity migration, interest multicast, consistent persistence) and a separate proof-of-concept.
- Voice: send *selected nearby participants'* audio via regional SFU (spatial mixing on client or where required), enforce mute/block/age restrictions before distribution. Never transmit every person's voice worldwide.

### Transport evaluation, not an automatic decision
| Candidate | Advantages | Cost/risks | Test |
| --- | --- | --- | --- |
| Existing engine UDP transport | fastest MVP integration; known game semantics | protocol and platform dependencies | PC ↔ Linux ↔ Quest, 5% loss |
| [GameNetworkingSockets](https://github.com/ValveSoftware/GameNetworkingSockets) | standalone open-source encrypted reliable/unreliable UDP message layer; portable non-Steam | C/C++ integration, deployment, separate game-state codec required | handshake, reconnect, fuzz, throughput |
| [QUIC DATAGRAM](https://www.rfc-editor.org/rfc/rfc9221.html) | standardized secure datagrams and reliable streams in one connection, connection management | datagram extension negotiation, congestion scheduling, protocol overhead; 0-RTT replay pitfalls | delayed/lost packets, MTU, mobile handover, anti-amplification |
| Bespoke bare UDP + crypto | full control | extreme security/maintenance risk | **not recommended for first release** |

Do not assume reliable *transport* makes a game rule secure. No client-originated purchase, inventory award, leaderboard, moderation sanction or achievement is trusted.

## 4. Anti-cheat & anti-abuse: server first, Linux-friendly

Threat cases and deterministic first checks:
| Attack or failure | Required server control | Evidence / false-positive guard |
| --- | --- | --- |
| Speed/fly/teleport | signed input sequence + server-side locomotion and allowed locomotion states | measure tracking anomalies, network correction and sanctioned accessibility modes |
| Weapon fire, score, item dupes | server checks cooldown/ammo/projectile/hit, transaction idempotency and unique ownership | store bounded event/replay trace and time sync |
| Replay, spoofed identity/session | short-lived audience/region/room-scoped one-time tickets, nonce consumption, server-only privilege checks | clock tolerance and reconnect semantics tested |
| Packet spam/DoS | per-action quotas, bounded message parser, admission/rate limits, early drop | measure false rejects under mobile jitter |
| Creator-circuit exploit | typed IR validator, script fuel/instruction cap, asset scanner, sandbox, resource budgets | explain failed graph nodes and appeal |
| Wallhack/ESP | only transmit state needed for legitimate gameplay, even if hidden by rendering | preserve spectators/admin legitimately authorized information |
| Aimbot/macros | server-side behavioral indicators + review; no single-signal automatic permanent ban | benign high-skill, latency and accessibility datasets |
| Abuse/report brigading | distinct safety moderation review, privacy-aware evidence, appeal and escalation | separate from cheating scores |
| Economy/marketplace fraud | server ledger, atomic commits, replay/idempotency, risk holds on payouts | audit events with retention & privacy controls |

- Never claim “unhackable” or 100% detection. Risk engine tiering: log → soft intervention → temporary gameplay restriction only with reliable evidence → human-reviewed ban, with appeals and documented retention/deletion.
- No intrusive kernel driver required in first release. Kernel anti-cheat would obstruct Linux/SteamOS and introduce device trust/privacy risks; evaluate only against actual demonstrated residual threats.
- Linux commercial middleware support **is not universal**. Steam's [Proton guide](https://partner.steamgames.com/doc/steamdeck/proton) describes per-title configuration for EAC/BattlEye; October 2026 coverage of EAC Linux v2 is evolving and cannot substitute for Metallic's own support test matrix.
- Security engineering: authorization at object/function level, signing keys in KMS, MFA for staff, scoped production access, rotating secrets, packet fuzzing, SBOM/supply-chain checks, authenticated service-to-service traffic, and manual approval for destructive operations.
- Protect player privacy: no covert recording, limited necessary telemetry, age-appropriate moderation safeguards and legal review across regions.

### Executable reference-only input checks

A **non-production, dependency-free** [reference input gate](../prototypes/authority/input_gate.py) and [unit fixtures](../tests/test_authority_input_gate.py) now demonstrate OS-neutral rejection of replay/duplicate sequence numbers, unexpected action types, invalid movement axes, input rates beyond per-tick caps, out-of-bounds normalized movement and unauthenticated session IDs. This is *not* a deployed anti-cheat or a server; it relies on a future authenticated transport to register sessions and a future engine simulator to enforce collision, physics, object permissions, hit detection, replay storage, lag compensation and anti-abuse policy. Because it never asks about the client's OS, a valid Linux user is checked the same way as a Windows user. It is a contract prototype to port into the selected game-server runtime only after profiling.

## 5. Self-hosted/regional infrastructure & capacity

**Stage A — local/team alpha:** 1 protected Linux test host + CI, dedicated server binary in a container, Postgres, Valkey, telemetry stack. Manual session allocator and fixed concurrency caps. Test data only; no public promises.

**Stage B — 100–1,000 CCU beta:** 2 independent Linux hosts in a region (plus managed DB/backup), deploy capacity-constrained sessions with reservations and explicit lifecycle. Multi-AZ only where provider offers meaningful failure isolation. Add a second region to prove auth/room compatibility and regional routing.

**Stage C — 10,000 CCU global planning goal:** regionally isolated, capacity-managed session fleets with prewarmed ready pools, health/heartbeats, per-zone limits, automated non-destructive draining, autoscaler hysteresis, queue/overload admission, build version pinning and CDN for room assets. Consider [Agones FleetAutoscaler](https://agones.dev/site/docs/advanced/scheduling-and-autoscaling/) and [Open Match](https://openmatch.dev/site/docs/guides/matchmaker/matchfunction/) only after first simple allocator limits are measured.

**Stage D — larger capacity:** hybrid bare-metal and cloud spillover if 24x7 reserved use economically justifies it; own rack/colo only with staffing, upstream transit, multi-carrier redundancy, remote hands, spare hardware, DR and 24/7 response. “Our own servers” can mean contractual control of leased bare metal; **owning data centers is not required.**

Regional strategy: select region using client RTT probe + real route health, party membership, legal/data constraints, version compatibility, capacity and price. A country/geolocation estimate alone is not an accurate latency measurement. When party members span continents, choose a fair compromise and transparently display predicted ping. Only route/replicate personal data cross-region where permitted.

**Failure isolation:** no direct game-server writes into global economy tables; service cache/queue outages must not corrupt balances. Split critical stores by domain and retain a source-of-truth Postgres ledger. Back up with encryption, tested restore and region-aware retention policies. Gracefully admit no new sessions if joining would jeopardize running rooms; preserve existing voice and game sessions across noncritical API restart.

### DDoS, transport and vendor fit (verify protocol and commercial availability)
- [Cloudflare Spectrum](https://developers.cloudflare.com/spectrum/) protects custom TCP/UDP *with appropriate paid/Enterprise arrangements*; ordinary HTTP reverse proxy does not make UDP safe.
- [Cloudflare Magic Transit](https://developers.cloudflare.com/magic-transit/about/) is an enterprise network-front-door option for dedicated IP range/colo via BGP/tunnels, not a simple switch for a home router.
- [AWS Shield Advanced](https://docs.aws.amazon.com/waf/latest/developerguide/ddos-resiliency-example-tcp-udp.html) has documented UDP game application patterns (EC2/EIP/Global Accelerator); check scope/cost/quotas and correct attached protection resources.
- [OVHcloud Game DDoS](https://docs.ovhcloud.com/en/guides/bare-metal-cloud/dedicated-servers/firewall-game-ddos) protects compatible gaming protocol profiles on eligible bare-metal products; a new custom Metallic UDP protocol may only receive generic rather than game-specific filtering until validated with provider.
- Encrypt transport and lock origins to approved ingress; avoid exposing game-server origin IPs via DNS, crash reports or client logs. Test recovery during mitigation as well as normal ping.
- **Do not** run unsanctioned high-volume traffic on production, other providers or third-party targets.

### Hosting metrics and cost truth
- Forecast using **measured peak CCU**, realistic average concurrent fraction, player egress kbps, SFU voice, CDN downloads, per-instance RAM/CPU, regional fragmentation, CPU/network headroom, persistent bytes and backups.
- 10,000 *hypothetical* simultaneous users × 96 kbps of world-state downstream ≈ **960 Mbps** before voice/protocol overhead; running that at constant load for 730 hours ≈ **315.36 decimal TB egress** before voice/overhead (960,000,000 bits/s × 730 h × 3600 / 8 / 10^12 = 315.36 TB). These are estimates, not Metallic metrics.
- Add voice and creator asset bandwidth separately; SFU multicast/voice subscriptions differ by channel and room.
- Per-region node count = ceil(required_sessions / conservatively measured safe_sessions_per_host), with at least one separate failure/rollout capacity reserve. Never use CPU theoretical maximum as safe production limit.
- No fixed dollar quote without actual region/provider SKUs, egress rules, utilization and quotes; use [AWS GameLift price and billing rules](https://aws.amazon.com/gamelift/servers/pricing/instance-pricing/) or provider quotes for date-stamped comparisons.
- See independent assumption-based helper in [tools/capacity_planner.py](../tools/capacity_planner.py). Its outputs are model projections, not a stress test.

## 6. Linux, Steam Deck, Windows and VR first-class support

Platform promises must follow runnable builds:
| Platform | Early support approach | Specific verification |
| --- | --- | --- |
| Windows x64 | Native signed client with DX11/DX12/Vulkan path selected after benchmark | clean install, keyboard/gamepad, 10/100 ms input latency |
| Linux x86_64 native | **Move earlier than P5 if feasible**; same protocol/session features, Vulkan (fallback validated), AppImage/tarball or other reviewed package | Ubuntu/Fedora on AMD/Intel/NVIDIA, Wayland + X11, locale/filesystem/case sensitivity |
| SteamOS/Steam Deck | Verify native Linux binary and separate Proton fallback; controller UI and performance profile | sleep/resume, Wi-Fi handover, keyboard input, verification checklist |
| PCVR OpenXR | Windows VR baseline; Linux PCVR only for runtimes/headsets actually tested | recenter, controller mappings, input/comfort and tracking |
| Meta Quest standalone | Android ARM64/OpenXR with mobile render tier and independent store certification | actual frame-time, thermals, asset memory, suspend/resume |
| Android phones | separate touch/UI/graphics and bandwidth profile | ARM64 devices GPU tiers, permissions, network switching |
| iOS / iPadOS | conditional staged port dependent on cost and store requirements | real devices, signing, privacy, backgrounding, memory |
| macOS | conditional native port | Apple Silicon hardware and API/render tests |
| Consoles | future licensed SDK/approval pipeline | cannot promise without developer programs, certification, and hardware |

**Explicit Linux acceptance:** build+sign/package native client in CI; login, join same Windows/Quest room, avatar rigs, chat, audio permissions, creator save/load, anti-cheat parity, spectator report and reconnect all verified against the same protocol version. Anti-cheat must not silently lock out native Linux.

OpenXR provides a common XR API but not universal guarantee of each vendor's extension/runtime features. [Khronos OpenXR](https://www.khronos.org/openxr/). Godot 4.6 export docs cover [Linux](https://docs.godotengine.org/en/4.6/tutorials/export/exporting_projects.html) and server resource filtering; Unity supports Linux [Dedicated Server builds](https://docs.unity.com/en-us/engine/6000.7/manual/platform-specific/dedicated-server/build). The engine decision must use measured native target builds.

### Client optimization playbook
- Target actual measured frametime, not just average FPS. For 72 Hz: 13.9 ms; 90 Hz: 11.1 ms per frame; include headroom for tracking/compositor and variability ([Meta performance guide](https://developers.meta.com/vr/documentation/unity/po-perf-opt-mobile/)).
- Asset tier budgets on materials, overdraw, lighting, shaders, VFX, avatars, skinned meshes, creator circuit CPU and total texture/video RAM. Quality tiers are **content contracts** required of published inventions.
- Streaming cells & HLOD in outdoor barn/hub, baking where appropriate, LOD for avatars, foveation/dynamic resolution where supported, shader variants prewarmed, async content fetching, avoid synchronized scene loads.
- VR: never trade away head-tracking comfort to save bandwidth; local head pose should be predicted/displayed immediately even as authoritative gameplay interactions are resolved by server.
- Linux native GPU telemetry/graphics APIs and per-vendor defects need separate CI/release tracking.

## 7. Reliability: limits, SLOs and incident response

Starter **proposed** beta service objectives (adjust only after measurement):
- Room join success in healthy regions: ≥99.0% over 28-day windows initially, excluding user cancellations but including service-side timeouts. Raise with evidence.
- Cached-assets join p95 < 5s in region under supported load; warm-room start vs cold start measured separately.
- Tick p95 below 70% of full fixed-step budget at target occupancy to preserve overload headroom; compare P99 too.
- No silent double-award of inventories/achievements; all financial and creator payouts require strict ledger correctness.
- No user-visible unrecoverable data loss in tested crash/restart scenarios within explicit save checkpoint window.
- Replay of any join ticket, stale action or signed asset manifest fails closed without locking valid reconnect.
- Public status page always separates observed regional errors from component guesses.

**Telemetry** (room/region/build/device tags; cardinality control): ccu_real, testbots, ready_sessions, allocated_sessions, orphan_sessions, admission_rejections, server_tick_seconds histogram, snapshot_bytes_out, relevant_entities_per_client, correction_distance, invalid_input_count, auth errors, region rtt histogram, joins p50/p95/p99, crash-free sessions, client CPU/GPU frametime percentiles, memory/thermals, voice SFU packets and consent state. Use [OpenTelemetry conventions](https://opentelemetry.io/docs/specs/semconv/) and [Prometheus histograms](https://prometheus.io/docs/practices/histograms/). Never label synthetic participants as real public users.

**Operational controls:** versioned deploy + canary, 5% → 25% → 100% gated rollout; auto rollback on error/tick regression, graceful room drain where possible, regional circuit breaker, signed server build enforcement, capacity limits, privileged break-glass access, audit logs, UPS/provider outage planning, recovery point (RPO) and recovery time (RTO) drills. On-call escalation must have humans for safety-critical and account-access issues.

## 8. Capacity experiment matrix and how to prove claims

**Run only against authorized private staging resources; block/rate-limit large tests until operator approval.**
- L0 unit/property/fuzz: invalid messages, bounds, replays, bogus ownership, decoder crashes, circuit budget exhaustion.
- L1 actual 2-person lobby and golf object with Windows ↔ native Linux ↔ Quest. Capture packet trace, corrected positions and game tick.
- L2 gradually 16/32/64 *real protocol* connected synthetic players; at least two real graphical clients to verify animation and visual effects.
- L3 100/500/1000 across **many real sessions** in one region, then distributed regions (not a single room); collect load generator saturation.
- L4 6h / 24h / 72h soak; restart allocator, kill session host, partition cache, disable object store, lose voice SFU, simulate 1%/5% packet loss and 30–250 ms jitter.
- L5 controlled regional failover, budget ceiling and DDoS vendor validation; never intentionally flood an unauthorized provider.
- L6 10,000 peak CCU only when L5 stable with cost bound, operational staffing, 20–30% tested spare capacity, concrete region throughput and signed builds.
- Test 1 room at 64, 64 rooms at 16, 1000 rooms at 10, and mixed reality to isolate O(n²) actor-visibility costs.
- Measure: concurrent real rooms, server P50/P95/P99 CPU tick time, instance RAM, outgoing kbps per recipient, QPS and DB waits, p99 join, disconnect/reconnect rate, signed-ticket rejection, client frametime on target hardware, test bot counts and cost / live player-hour.

**Competitive-performance claims protocol:** reproduce the *same* activity type, similar room density, hardware and network conditions when comparing to archived Rec Room recordings/builds; disclose that current service cannot be live-benchmarked after shutdown. Never claim “faster than Rec Room” solely from code microbenchmarks.

## 9. First four implementable milestones

### P0 — portable benchmark specification + executable model
- [ ] Commit this research, sample capacity scenario and capacity calculator with unit tests (model, **not a benchmark**).
- [ ] Publish one standardized performance-trace JSON schema with build commit, player type, room occupancy, hardware, driver, network RTT/loss and measured histograms.
- [ ] Record ADR: Unity NGO vs Entities vs Godot XR, and transport baseline (engine vs GameNetworkingSockets vs QUIC).

### P1/P2 — *actual* independently hosted session
- [ ] Native Linux dedicated server and Windows client, signed temporary join ticket with nonce and audience.
- [ ] Shared same-world session from Linux client and PCVR, server authoritative input, prefab and player movement.
- [ ] Central validation for interactions/circuits, false-positive anti-cheat test fixtures and replay logging with retention limits.
- [ ] Real protocol bot harness (existing issue #39), 16/32 players over WAN, measured RTT/tick/bytes.

### P5 — Linux and VR parity as a quality gate
- [ ] Native Linux client with test matrix and signing/packaging; Proton path separately verified.
- [ ] Quest controller/headset frame-time captures, Linux Wayland/X11 and SteamOS sleep/resume tests.
- [ ] Platform security features are not allowed to exclude supported Linux devices without a published supported fallback.

### P6 — regional capacity / control plane
- [ ] Two geographically separate regional fleets and allocation; test graceful drain, replay-free reassignment and edge protection.
- [ ] Real SLO dashboards, tested restore and load-based warm-pool.
- [ ] Cost model replaced with measured per-room CPU/memory/network usage and provider egress/SFU rates.
- [ ] Only upgrade to 10k-CCU advertised readiness after signed results, sustained real-protocol staging load and on-call plan.

## 10. Sources / decision evidence

Primary technical documents reviewed October 18, 2026:
1. [Agones overview](https://agones.dev/site/docs/overview/), [GameServerAllocation](https://agones.dev/site/docs/reference/gameserverallocation/), [autoscaling](https://agones.dev/site/docs/advanced/scheduling-and-autoscaling/); effective for on-prem and cloud orchestration once fleet complexity exists.
2. [Open Match match function](https://openmatch.dev/site/docs/guides/matchmaker/matchfunction/); customizable matchmaking candidate, not mandatory for MVP.
3. [Unity Dedicated Server](https://docs.unity.com/en-us/engine/6000.7/manual/platform-specific/dedicated-server/build) and [netcode choices](https://docs.unity.com/en-us/multiplayer/netcode/netcode).
4. [Valve GameNetworkingSockets](https://github.com/ValveSoftware/GameNetworkingSockets) and [IETF RFC 9221 QUIC DATAGRAM](https://www.rfc-editor.org/rfc/rfc9221.html); test before adopting.
5. [Nakama authoritative multiplayer](https://heroiclabs.com/docs/nakama/concepts/multiplayer/authoritative/); valid backend alternative if measured fit warrants it.
6. [LiveKit self-hosting](https://docs.livekit.io/transport/self-hosting/) and [multi-region](https://docs.livekit.io/transport/self-hosting/distributed/).
7. [Cloudflare Spectrum TCP/UDP requirements](https://developers.cloudflare.com/spectrum/) and [Magic Transit](https://developers.cloudflare.com/magic-transit/about/); [OVH game firewall](https://docs.ovhcloud.com/en/guides/bare-metal-cloud/dedicated-servers/firewall-game-ddos); [AWS Shield TCP/UDP architecture](https://docs.aws.amazon.com/waf/latest/developerguide/ddos-resiliency-example-tcp-udp.html).
8. [Valve Steamworks Proton anti-cheat](https://partner.steamgames.com/doc/steamdeck/proton); per-title Linux compatibility required.
9. [Khronos OpenXR](https://www.khronos.org/openxr/); [Godot 4.6 exports](https://docs.godotengine.org/en/4.6/tutorials/export/exporting_projects.html); [Meta Quest frame performance](https://developers.meta.com/vr/documentation/unity/po-perf-opt-mobile/).
10. [OWASP API Top 10 (2023)](https://api-security.owasp.org/editions/2023/en/0x11-t10/), [OpenTelemetry semantics](https://opentelemetry.io/docs/specs/semconv/), [Google SRE SLO guide](https://sre.google/workbook/implementing-slos/).
11. [Valve Source multiplayer networking](https://developer.valvesoftware.com/wiki/Source_Multiplayer_Networking), [Gaffer snapshot compression](https://www.gafferongames.com/post/snapshot_compression/) explain performance/fairness engineering, not copyable code.

**What the document does not do:** deploy a host, run performance benchmarks, authorize providers, ship a binary, pay for infrastructure, or establish a definitive numeric advantage. Complete work with verified test evidence and use the exact same review criteria for any claim of being faster, safer, or more compatible than historical alternatives.
