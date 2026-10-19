# Metallic-RC — Performance, Synthetic Players and Continuous Quality Laboratory

**Research baseline:** 2026-10-15 · **Status:** IMPLEMENTATION PLAN, not a demonstrated player capacity.
**Goal:** reproducibly prove backend robustness, responsive VR and desktop graphics, creator-scene correctness, moderation resilience and excellent player experience before release.

## Why one kind of test is not enough

A scene running at 90 FPS with two Unity instances does not prove 1,000 players can connect. 1,000 Python HTTP clients do not prove VR animation/physics is correct. A high-performing test on the owner's LAN cannot establish performance in multiple regions. Test each layer in isolation and in end-to-end combination.

| Layer / test lane | Test workload | Tool candidates | Measured evidence |
| --- | --- | --- | --- |
| API protocol | register/login, profiles, room list, join-ticket creation, moderation and creator uploads | [Locust](https://docs.locust.io/), [Grafana k6](https://grafana.com/docs/k6/latest/) | requests/sec, error ratio, p50/p95/p99, saturation |
| Realtime protocol | many simulated authenticated clients sending snapshots/input and receiving deltas | project-specific **headless game client** / Unity thin-clients where appropriate | active sessions, accepted tick rate, net KB/s/player, disconnects |
| Deterministic in-game agents | navigation, interacting, collecting, building, reporting, changing rooms | finite-state test drivers, Unity [ML-Agents](https://github.com/Unity-Technologies/ml-agents), scripted playtest agents | objective completion, stuck states, unexpected transitions |
| Visual/VR clients | full rendering, animation rigs, VFX, spectator presence | real PC, Quest and mobile hardware + Unity Profiler, Frame Debugger | GPU/CPU frame time, memory, thermal, comfort and graphical correctness |
| Voice/network realism | Opus, SFU, packet loss/jitter/latency, muted and blocked states | SFU synthetic media tracks, network simulator, tc/netem in isolated lab | MOS proxy/packet loss, jitter, voice route CPU, privacy |
| Admin/moderation | permissioned sanctions, appeals, token gates, audits, Discord queues | authorization matrix/unit and integration tests, UI E2E | wrong-role denial, audit completeness, revocation propagation |
| Security/recovery | region down, expired ticket, DB restart, secret rotation, storage outage | fault injection, staging backups, chaos drills | RTO, RPO, data invariants and graceful degradation |
| Creator scalability | hundreds of placed objects, 1000 circuit events/tick, multi-user edits | synthetic scene packages and graph conformance harness | creator save/undo consistency, instruction budget, RAM/tick |
| Release experience | installer, login, room join, content streaming, platform certification | clean-machine installer tests and Meta VRC checks | time to first world, upgrade recovery, platform certification |

**Tool facts:** Unity [Multiplayer Play Mode](https://docs.unity.com/en-us/multiplayer/netcode/networking-utilities) supports up to four simultaneous players locally in its documented workflow, so it is not a production-scale load generator. Unity Netcode for Entities can also use **thin clients** that generate input without rendering ([Unity documentation](https://docs.unity.cn/Packages/com.unity.netcode@1.0/manual/playmode-tool.html)). [Locust](https://docs.locust.io/) distributes HTTP/protocol workers; [k6](https://grafana.com/docs/k6/latest/using-k6/protocols/websockets/) supports WebSocket testing. These are complementary rather than alternatives to game-wire load clients. [Unity ML-Agents](https://github.com/Unity-Technologies/ml-agents) supports learning agents and automated game build tests.

## Metallic Test Lab — four types of synthetic player

1. **API Users (100s–10,000s per orchestrated staging environment):** register test accounts, login, browse, join, disconnect and reconnect. They have no renderer and cannot prove avatar/network performance. Use Locust and k6 with dedicated test credentials and rate ceilings.
2. **Protocol/Thin Players (10s–1,000s when hardware permits):** connect to an **independent staging game server** using the *actual* shipping transport and client protocol. Generate plausible input movement, hand poses, interactions and chat/voice opt-in. Validate world state and packet recovery. They do not imply Quest GPU frame-rate.
3. **Behavior AI Players (4–100 or more if measured):** game-aware agents that traverse quests, interact, jump, test collisions, spam permitted non-destructive UI operations, exercise maker tools and report discoverability. Begin with reproducible finite-state machine bots before adding trained ML-Agents. AI is useful for exploration, but deterministic seeds are mandatory for regression reproduction.
4. **Real Rendered Clients (2–32 depending on actual GPU budget):** real PC/Quest/mobile instances for visuals, animation, VFX, physics feel, sound and comfort. Must include real-headset tests because a synthetic bot cannot validate vestibular comfort.

Never merge these into a single claim like “10,000 in-game players verified.” Report **API virtual users**, **connected simulated gameplay sessions**, **AI agents** and **real hardware clients** separately.

### Synthetic traffic contract

Every test agent:
- uses a **test-only identity namespace**: \`testbot_<run_id>_<ordinal>\`, with \`isSynthetic = true\` and scoped test permissions;
- carries a signed staging-only test token and optional \`X-Metallic-Test-Run\` trace identifier;
- never receives creator payments, leaderboard rankings, volunteer moderator/developer roles, achievements or an ordinary social account profile;
- never contributes to **public players online** or Discord public activity counts;
- joins only private test rooms/servers not indexed in production discovery;
- follows a randomized but seeded pacing profile (idle, move, jump, interact, emote, open/watch menu, leave/rejoin);
- reports timing/packet outcomes without storing live users' private conversations;
- stops automatically when time, concurrency, CPU, budget or error thresholds exceed run policy;
- is constrained by allowlisted **owned staging destinations** and an approval gate for heavy runs.

## Reference scenario mix (adjust after telemetry)

| Scripted actor | Fraction | Operations | Failure checks |
| --- | ---: | --- | --- |
| Casual social | 30% | join, stand, move, friend invite in test scope, emote | presence TTL, state replication |
| Explorer | 20% | browse rooms, join/rejoin, waypoint motion | room switching, view distance |
| Builder | 15% | place, rotate, undo, save draft in synthetic room | consistency, object budgets |
| Circuits player | 10% | trigger controls, timers, quests, scoring | graph determinism and throttles |
| Frequent reconnect | 10% | intentional disconnect, renew ticket, resume | ghost player cleanup, replay protections |
| Chat/voice tester | 5% | opt-in sample media only; permitted test messages | mute/block, SFU bandwidth |
| Marketplace tester | 5% | synthetic transactions in fake-currency ledger | idempotency, double spend resistance |
| Moderator tester | 5% | fake reports, staff actions with fixtures | RBAC, audit, appeals |

**Important:** percentages are example planning weights. Run each scenario solo first to isolate cost, then use the mix. Never use real funds or external payment test accounts without explicit sandbox credentials.

## Test levels and success criteria

**L0 — Pull request (CI, fast):** schema tests, authorization tests, deterministic simulation replay, unit logic, widget/UI screenshot checks; a few *localhost* synthetic players against an ephemeral fixture. Hard fail on test regression.

**L1 — Daily shared staging (controlled):** 10→50→100 simulated session load if service exists, 15–30 minute soak, network fault profiles. Trend p95 of join, per-tick CPU time, packet drops and memory.

**L2 — Weekly planned staging load:** 100→250→500→1,000 clients **only if** measured capacity and upstream policy support it. Ramp rate gradually, sample multiple regions, reset isolated test data afterward.

**L3 — Pre-beta review:** 6-hour/24-hour soak, full fake voice media routing (only synthetic media), forced restarts, rollback tests, failover. Measure cold-region provision, DB restoration and memory fragmentation.

**L4 — Public-beta operational capacity:** measured real load plus approved off-peak stress on dedicated test fleet. Stop immediately if public endpoints are impacted. Do not generate floods on unrelated third-party services.

**L5 — Release acceptance:** validate VRC requirements on every advertised Meta Quest target, device thermal soak, gameplay test coverage, moderation permissions and accessibility audit. Publish exact build + device + capture data.

**Metrics and proposed gates (targets to refine):**
- Server tick time p95 ≤ target frame budget derived from selected rate; e.g. at 30 Hz, entire tick must stay under **33.3 ms** at sustained supported peak, with headroom.
- Client VR frame time p95 ≤ headset's per-frame period: 13.9 ms at 72 Hz, 11.1 ms at 90 Hz; no repeated multi-frame stalls under realistic content.
- Authentication and room join p95: define per-region SLO; proposed **<5 s** room join after assets cached on a healthy region.
- At target concurrency, test error rate for game joins <1%; production reliability SLO requires a separate baseline and outage accounting.
- Creator save/load uses a unique versioned commit ID; exact rollback and conflict tests pass.
- No economy ledger imbalance after concurrent fake purchases.
- Missing/replayed/expired join tickets rejected while legitimate reconnect survives.
- Every privileged staff operation has an immutable audit receipt, reason and actor.

Gate explicitly on *measured headroom*, not merely surviving traffic for 30 seconds. Measure server CPU, memory and network **and** traffic generator CPU/queue backlog. Distributed load can otherwise under-generate requests without obvious warning.

## Capacity mathematics and honest player numbers

\`estimated_game_egress_Mbps = 0.001 * users * measured_kbps_per_user\`

\`estimated_max_sessions = min(cpu_budget/tick_cpu_per_session, memory_budget/RAM_per_session, egress_budget/egress_per_session)\` — only with real measurements and a safety factor.

A simple example, **not a measured Metallic result**: 1,000 users × 80 kbps world-state outbound ≈ 80 Mbps and ~25.9 TB outbound for a 30-day 100% duty cycle; additional SFU voice, CDN content, protocol overhead and inbound traffic are excluded. A single hot room with 100 visible avatars can be very different from 10 small rooms of 10 users because actor update density can approach O(n²) without interest management.

Budget test:
- empty room baseline per tick and heap;
- 16/32-player room with 1/4/16 tracked objects each;
- 2/16/64 avatars in shared visibility sphere;
- 1/10/100 circuits nodes triggered per actor;
- streaming 10 MB/50 MB/200 MB original room bundle;
- 10/50/100 concurrent voice participants per media SFU node, where allowed;
- 1/4/16 simultaneous creator scene authors.

For each, record CPU ms, GPU ms, RSS, allocated bytes/frame, network packets/bytes, DB QPS and retries, median/p95/p99.

## Simulation engines and AI playtest agents

Build an adapter around the shipping game's input path rather than manipulating database rows:
- **Controller:** seeded FSM with states \`Idle -> Explore -> Interact -> Wait -> SwitchRoom\`; deterministic event log so failures can be replayed.
- **Sensors:** position, collision, nearest interactable, quest state, RPC errors, circuit events and UI focus.
- **Actions:** move vector, turn, jump, press/grab, build object, change outfit, invite test party, opt-in mute.
- **Assertions:** object moved to expected position, timer completed, avatar rendered on real client, no server crash, correct role-denial response.
- **Stress personality:** idle, spammy-but-rate-limited, reconnecter, explorer, builder, quest competitor and pathfinding edge-case explorer.
- **Optional ML-Agents:** train policies to find stuck rooms and weird interactions, but validate each discovery by deterministic replay. Deep-RL models can become unreliable after content changes and should not replace basic scripted test suites.

### Failure injection matrix

| Fault | Expectation |
| --- | --- |
| Packet loss (1%, 5%, 10% synthetic) | No crash; bounded world-state divergence; meaningful connection indicator |
| Latency/jitter (30–250 ms profiles) | Interpolation and responsiveness degrade gracefully |
| Auth service restart | Existing game sessions remain stable within token validation cache policy |
| Room host process crash | Clients return to lobby and can rejoin/recover checkpoint when supported |
| Valkey/cache unavailable | Cache miss fallback; no authoritative economy writes in cache |
| PostgreSQL brief outage | Transactions not double-spent, idempotent retry, degraded status visible |
| Object storage temporarily down | Existing cached room continues; new loads explain failure |
| SFU media node restart | Voice reconnection; muted/blocked state reasserted |
| Region saturated | Matchmaker stops accepting new joins and routes to healthy region if possible |
| Broken client asset manifest | Signed update rejected and launcher offers repair/rollback |
| Creator publishes invalid circuit | Reject safely with useful node-level diagnostics |
| Discord API outage | Game remains playable, bot reports stale state and retries with backoff |

## Visualization, VFX and animation stress testing

Stress scenes require multiple variants of:
- **VFX:** pooled particles, impacts, fog, volumetrics, emissive materials, sparks, projectile trails; varied emitters on/off-screen; lower-tier safe path for Quest/mobile; clean fallback when VFX budget hit.
- **Animation:** 2/16/64 rigged avatars; IK recalculation; simultaneous emotes; item hand attachment; full-body tracking, blend-tree transitions; facial expressions.
- **Environment:** transparent geometry, multiple local lights, reflective metallic surfaces, baked versus realtime shadows, CPU physics ragdolls, UI canvases and large creator asset collections.
- **UI:** open all panes with intentionally long names, status banners, concurrent notifications, translated strings, hover/focus states and hundreds of catalog entries.
- **Creator:** thousand-instance library with progressive streaming and valid material instances; high-frequency edit/undo from multiple clients.

Visual gate: pixel diff on baseline screenshot **plus** human review (automated comparison alone cannot judge aesthetic quality); render hardware reports on Quest/PC and static/animated layout checks for scale and safe margins. Animations must respect reduced motion, comfort and photosensitivity options.

## Test-lab operating architecture

\`\`\`mermaid
flowchart TB
  CI["GitHub CI + scheduled controlled test"]
  AP["Authorized lab control API"]
  FS["Scenario and run manifest (version/seed)"]
  LC["Locust / k6 API users"]
  TP["Headless/thin protocol bots"]
  AI["FSM and ML-Agents game bots"]
  UI["Real PC / Quest / phone clients"]
  AUTH["Synthetic identity service"]
  STAGE["Private staging / simulated rooms"]
  OBS["OpenTelemetry / Prometheus / Grafana"]
  ART["Immutable test evidence (build, input seed, graph, traces)"]
  CI --> AP
  AP --> FS
  FS --> LC
  FS --> TP
  FS --> AI
  FS --> UI
  LC --> AUTH
  TP --> AUTH
  AI --> AUTH
  AUTH --> STAGE
  UI --> STAGE
  LC --> STAGE
  TP --> STAGE
  AI --> STAGE
  STAGE --> OBS
  OBS --> ART
  ART --> CI
\`\`\`

**Orchestrator API (future):** \`POST /internal/test-runs\` with approved \`scenario_id\`, capped \`users\`, \`duration_seconds\`, target **staging** region, deterministic \`seed\`, and a privileged test principal. \`GET /internal/test-runs/{id}\` gives progress and health; \`POST /internal/test-runs/{id}/cancel\` must stop bots cleanly. Destructive tests require explicit separate approval.

### Example test run metadata

\`\`\`json
{
  "schema": 1,
  "runId": "synthetic-2026-10-15-001",
  "environment": "owned-staging",
  "scenario": "mixed-social-and-creator",
  "seed": 424242,
  "apiVirtualUsers": 100,
  "protocolPlayers": 32,
  "renderedClients": 2,
  "maxDurationSeconds": 900,
  "publicPresenceIncluded": false,
  "testOnlyAccounts": true,
  "capturedMetrics": ["auth_p95", "room_join_p95", "tick_p95", "bandwidth_mbps", "disconnect_rate"]
}
\`\`\`

This is a **specification example**; no real session is running.

## Reference tooling to evaluate

- [Locust, distributed](https://docs.locust.io/en/stable/running-distributed.html) — API traffic and session lifecycle, Python tasks and load dashboard.
- [Grafana k6 + WebSockets](https://grafana.com/docs/k6/latest/using-k6/protocols/websockets/) — repeatable API/WS scenario and threshold gates.
- [Unity Multiplayer Tools](https://docs.unity.com/en-us/multiplayer/netcode/networking-utilities) — profiler/network emulator and small multi-instance validation.
- [Unity thin clients](https://docs.unity.cn/Packages/com.unity.netcode@1.0/manual/playmode-tool.html) — input generators without renderer in the Entities toolchain; verify compatibility with chosen package version.
- [Unity ML-Agents](https://github.com/Unity-Technologies/ml-agents) — automated gameplay testing and policy learning.
- [Agones autoscaling](https://agones.dev/site/docs/reference/fleetautoscaler/) — optional fleet allocator/scaler after actual orchestration demands it.
- [OpenTelemetry](https://opentelemetry.io/) — metrics and traces; [Grafana](https://grafana.com/oss/grafana/) — dashboards.
- [Meta Horizon VRC performance checks](https://developers.meta.com/vr/resources/publish-quest-req/) — mandatory release QA for Meta Quest.

## Next buildable tasks

1. Implement a **localhost fixture server + safe seeded mock bot harness** in \`tests/load/\`; use it to prove scenario generation and metrics *only*.
2. Introduce one actual game client wire protocol and add protocol-aware headless agents as soon as the authoritative session server exists.
3. Gate all load tools behind staging allowlists and traffic/cost limits. Make it impossible for test accounts to inflate public presence.
4. Add dashboards for server tick time, queue, allocs, bandwidth, join p95, server error rates and test scenario IDs.
5. Add real Quest/mobile automated capture and creator-scene benchmark suites once the engine is selected.

**Never represent mock-player load as verified game-server capacity or as actual players online.**
