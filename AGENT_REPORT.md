# AGENT_REPORT — Issue #37: Trusted production heartbeat player-count aggregator

**Branch:** `agent/bigpickle-37`
**Commit:** `c5c07d8` (implementation) — see also the follow-up report commit
**Status:** Implemented, tested, committed to the branch. **Not pushed. `main` untouched.**
**Reality check:** no room server, account service or game client exists in this
repository, so this service has **no live production player count**. It is real,
tested code that implements the contract, not a deployed data source.

---

## 1. Task

From `gh issue view 37 -R oncoursemail/Metallic`:

> [P1] Build trusted production heartbeat player-count aggregator
> Service aggregates valid, de-duplicated real player sessions and independently
> tracks synthetic test actors with TTL.

Acceptance tests:

- [x] API rejects stale counts, bot counts, test identities and unauthorized requests.
- [x] Reproducible code/build/test evidence and passing CI.
- [x] Document limitations, supported devices and risks.

## 2. What was built

A new service, `services/player-count/`, that is the **producer** behind the
aggregate already consumed by the Discord bot
(`services/discord-bot/src/status.mjs`).

| File | Purpose |
| --- | --- |
| `services/player-count/src/aggregator.mjs` | Pure, dependency-free aggregation core: heartbeat parsing/validation, de-duplication, synthetic-actor separation, TTL expiry. |
| `services/player-count/src/server.mjs` | Dependency-free `node:http` surface: authenticated ingest + read, constant-time bearer auth, bounded bodies, stale-safe read. |
| `services/player-count/src/main.mjs` | Entry point wiring environment variables to the server, with graceful shutdown. |
| `services/player-count/tests/aggregator.test.mjs` | 11 unit tests for the core logic (incl. a cross-service contract test). |
| `services/player-count/tests/server.test.mjs` | 8 HTTP tests over a real loopback server. |
| `services/player-count/README.md` | Contract, acceptance mapping, limitations, supported devices/runtimes, risks. |
| `services/player-count/package.json` | `npm start` / `npm test` scripts, Node 22+. |
| `.github/workflows/update-progress.yml` | New paths trigger + syntax checks + `node --test` for the service in CI. |
| `README.md`, `services/discord-bot/README.md` | Cross-references to the new service. |

### Design / behaviour

- **Heartbeats are full-room snapshots** from authenticated room servers:
  `POST /internal/room-heartbeats`. Re-sending cannot inflate the count;
  `sequence` must strictly increase per room (monotonic, out-of-order rejected).
- **Real players are de-duplicated by `accountId`** across duplicate sessions,
  multiple devices and multiple rooms.
- **Synthetic `testbot_*` actors are tracked in a separate set** with the room
  TTL and are never added to the public number.
- **Fail-closed identity checks**: a session claiming to be a real player but
  carrying a `testbot_` account id, or an `actorId`, is rejected wholesale with
  `422 test_identity_in_real_sessions`; unauthenticated "real" sessions and
  malformed synthetic sessions are rejected (`400 invalid_heartbeat`).
- **Staleness enforced both ways**: stale/future heartbeats are refused on
  ingest (`409`), and `GET /v1/aggregate/players` returns `503` when no room is
  within its TTL — never a zero-filled or stale count.
- **Auth**: constant-time bearer comparison; **separate ingest and read tokens**
  so a status consumer cannot publish fake counts. Both endpoints require it.
- **Privacy**: the public aggregate exposes only counts, `observedAt`,
  `ttlSeconds`, `environment` and `source` — never account/session ids or rooms.

HTTP surface:

| Method | Path | Auth | Purpose |
| --- | --- | --- | --- |
| `POST` | `/internal/room-heartbeats` | ingest token | Accept one room heartbeat |
| `GET` | `/v1/aggregate/players` | read token | Public aggregate snapshot |
| `GET` | `/healthz` | none | Liveness |

## 3. Acceptance mapping

| Acceptance test | Evidence |
| --- | --- |
| API rejects stale counts | `stale_heartbeat`, `future_heartbeat`, `out_of_order`, `503 stale_no_fresh_heartbeats` tests |
| API rejects bot counts / test identities | `test_identity_in_real_sessions` test; separate `syntheticTestPlayers` |
| API rejects unauthorized requests | ingest + read `401` tests (missing/wrong/cross token) |
| De-duplicated real sessions | de-dup by account across sessions, devices and rooms |
| Synthetic actors tracked with TTL | separate synthetic set + TTL expiry tests |
| Reproducible code/build/test evidence | commands below; CI wiring |
| Document limitations / devices / risks | `services/player-count/README.md` |

## 4. Test evidence (reproducible)

Commands (run from the repository root):

```shell
# new service
node --test services/player-count/tests/*.test.mjs

# regression: everything that CI runs
node --check services/player-count/src/aggregator.mjs
node --check services/player-count/src/server.mjs
node --check services/player-count/src/main.mjs
node --test services/discord-bot/tests/*.test.mjs services/staff/tests/*.test.mjs services/player-count/tests/*.test.mjs
python -m unittest discover -s tests -p 'test_*.py'
```

### New service suite (19/19 pass)

```
✔ counts each authenticated account once across duplicate sessions and devices
✔ de-duplicates one account present in several rooms
✔ tracks synthetic test actors separately and never counts them as real
✔ room counts expire with the heartbeat TTL
✔ prunes expired rooms instead of leaking memory
✔ rejects stale, future and out-of-order heartbeats
✔ rejects bot and test identities claiming to be real players
✔ rejects unauthenticated, malformed and wrong-environment heartbeats
✔ refuses new rooms once the configured capacity is reached
✔ aggregate snapshot satisfies the Discord status contract exactly
✔ a fully stale snapshot is reported unavailable, never as zero online
✔ health probe is public and reports the environment
✔ unknown routes and wrong methods are rejected
✔ ingest and read both reject unauthorized requests
✔ a valid heartbeat is accepted and the aggregate is served
✔ stale counts, bot identities and out-of-order snapshots are rejected
✔ malformed and oversized bodies are rejected
✔ read reports unavailable instead of a stale or zero count
✔ the server refuses to start without strong distinct tokens
ℹ tests 19  pass 19  fail 0
```

### Existing suites (unchanged, still green)

```
node --test services/discord-bot/tests/*.test.mjs services/staff/tests/*.test.mjs services/player-count/tests/*.test.mjs
ℹ tests 31  pass 31  fail 0

python -m unittest discover -s tests -p 'test_*.py'
Ran 19 tests ... OK
```

### End-to-end smoke of `src/main.mjs` (real loopback HTTP)

```
HEALTH: ok env=production
INGEST: accepted=True real=1 synthetic=1
AGGREGATE: humans=1 bots=1 source=server-heartbeat-aggregator
UNAUTH READ STATUS: 401
```

The `aggregate snapshot satisfies the Discord status contract exactly` test
imports `parseProductionStatus` from the Discord bot and asserts the aggregate is
accepted — proving the producer/consumer contract stays compatible.

## 5. CI

`.github/workflows/update-progress.yml` now:

- triggers on `services/player-count/**`;
- runs `node --check` on the three source files;
- runs `node --test services/player-count/tests/*.test.mjs`.

CI cannot be observed from this worktree (no push allowed), but the exact
commands the workflow runs were executed locally and pass. The service has no
third-party dependencies, so no install step is required.

## 6. Limitations, supported devices and risks

(Also documented in `services/player-count/README.md`.)

**Limitations**

- **No real data.** No room server / account service / game client exists here,
  so there is no live count. All numbers are test fixtures or schema examples.
- **In-memory only.** State is process-local and lost on restart; no persistence,
  migrations, or multi-instance sharing yet.
- **String validation is not attestation.** Authenticity relies on the
  authenticated transport and service-to-service tokens.
- **Clock trust.** Freshness/ordering/skew limits assume reasonably synced clocks.
- **No per-room rate limiting / anomaly detection yet** (only body-size and room
  capacity caps).

**Supported devices / platforms**

- **Service runtime:** Node.js 22+ on Windows, Linux or macOS; CI uses
  `ubuntu-latest`; no native modules or external dependencies.
- **Game clients counted:** none yet. The aggregator is platform-agnostic and is
  not tied to any PC/VR/mobile client in this repository.
- **Future deployment:** behind the protected HTTPS edge
  (`docs/ARCHITECTURE.md` §10); never exposed directly to game clients.

**Risks**

- A compromised ingest token could push noisy data (mitigated by separate tokens,
  validation and caps, but no WAF/rate limiting inside this service).
- De-dup policy is one-account-once; "multiple devices count more than once" is
  intentionally unsupported.
- Counts must never be published as live until a trusted room-server fleet and
  protected transport exist. The Discord bot must keep reporting *unavailable*.

## 7. Not implemented / next steps

Persistence + migrations, horizontal scaling, per-room rate limiting, alerting,
TLS/edge termination, room-server heartbeat emitters, account-service
integration, per-region rollups, deployment. Environments and secrets are read
from environment variables only; no secrets are committed.
