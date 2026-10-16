# Metallic Studios — Trusted Production Player-Count Aggregator

**Implementation status:** local, tested, **not deployed**. No authoritative room
server or real game telemetry exists in this repository yet, so this service
currently receives no production heartbeats. It exists to make the
player-count contract real, authenticated and covered by tests before the game
backend ships.

This service is the producer behind the aggregate that the
[Discord bot status module](../discord-bot/src/status.mjs) consumes. It turns
authenticated room-server heartbeats into an independently validated count of
**real, de-duplicated, authenticated players**, while **synthetic test actors are
tracked separately and never added to the public number**.

Related design: [Moderation, Discord and staff](../../docs/MODERATION_DISCORD_STAFF.md)
and [Quality and stress testing](../../docs/QUALITY_AND_STRESS_TESTING.md).
Task: [#37](https://github.com/oncoursemail/Metallic/issues/37).

## What is implemented today

- Room-server heartbeat ingestion over an authenticated HTTP endpoint.
- Strict validation: schema, source, environment, timestamp, TTL, sequence and
  per-session identity/authentication.
- De-duplication of real players by **account id** across duplicate sessions,
  multiple devices and multiple rooms.
- Separate, TTL-bounded tracking of synthetic (`testbot_*`) actors.
- Staleness enforcement in both directions: stale/future heartbeats are refused
  on ingest, and the read endpoint returns `503` (never a zero-filled count)
  when no room is within its TTL.
- Rejection of bot counts and test identities that try to appear as real
  players.
- Constant-time bearer authentication with **separate ingest and read tokens**.
- Bounded request body size; the public aggregate exposes counts only, never
  account ids, session ids or room names.

Source: [src/aggregator.mjs](src/aggregator.mjs) (pure logic) and
[src/server.mjs](src/server.mjs) (HTTP). Entry point: [src/main.mjs](src/main.mjs).

## HTTP contract

| Method | Path | Auth | Purpose |
| --- | --- | --- | --- |
| `POST` | `/internal/room-heartbeats` | ingest token | Accept one full-room heartbeat |
| `GET` | `/v1/aggregate/players` | read token | Public aggregate snapshot |
| `GET` | `/healthz` | none | Liveness only |

### Heartbeat (room server → aggregator)

```json
{
  "schema": 1,
  "source": "room-server",
  "environment": "production",
  "roomId": "room-abc",
  "region": "eu-west",
  "observedAt": "2026-10-15T22:00:00Z",
  "ttlSeconds": 60,
  "sequence": 12,
  "sessions": [
    { "sessionId": "s-1", "accountId": "acct-1", "actorId": null, "authenticated": true, "synthetic": false },
    { "sessionId": "s-2", "accountId": null, "actorId": "testbot_run_001", "authenticated": false, "synthetic": true }
  ]
}
```

- A heartbeat is a **full snapshot** of one room, so re-sending it cannot inflate
  the count. `sequence` must strictly increase for a given room.
- Real sessions require `authenticated: true` and a non-`testbot_` `accountId`.
- Synthetic sessions must use the reserved `testbot_` actor namespace, carry no
  real `accountId` and must not claim real authentication.

### Rejection codes

| Status | `error` | Meaning |
| --- | --- | --- |
| `400` | `invalid_heartbeat` / `invalid_json` / `wrong_environment` | Malformed, wrong schema/source or wrong environment |
| `401` | `unauthorized` | Missing or wrong bearer token |
| `409` | `stale_heartbeat` / `future_heartbeat` / `out_of_order` | Timestamp outside TTL/skew, or not newer than the last snapshot |
| `413` | `payload_too_large` | Body over 1 MiB |
| `422` | `test_identity_in_real_sessions` | A bot/test identity tried to count as a real player |
| `503` | `stale_no_fresh_heartbeats` | Read requested with no room within its TTL |

### Aggregate (aggregator → Discord bot / dashboards)

Matches the contract in [status.mjs](../discord-bot/src/status.mjs) exactly (a
contract test asserts this):

```json
{
  "schema": 1,
  "environment": "production",
  "fresh": true,
  "realAuthenticatedActivePlayers": 2,
  "syntheticTestPlayers": 1,
  "activeRoomSessions": 1,
  "observedAt": "2026-10-15T22:00:00Z",
  "ttlSeconds": 60,
  "source": "server-heartbeat-aggregator"
}
```

## Acceptance mapping (issue #37)

| Acceptance test | Where it is proven |
| --- | --- |
| API rejects stale counts | `stale_heartbeat`, `future_heartbeat`, `out_of_order`, `503 stale_no_fresh_heartbeats` |
| API rejects bot counts / test identities | `test_identity_in_real_sessions`, separate `syntheticTestPlayers` |
| API rejects unauthorized requests | constant-time bearer auth on ingest and read; `401` tests |
| De-duplicated real sessions | de-dup by account id across sessions/devices/rooms |
| Synthetic actors tracked with TTL | separate `syntheticTestPlayers` set with room TTL expiry |
| Reproducible code/build/test evidence | `node --test services/player-count/tests/*.test.mjs`; CI in `update-progress.yml` |
| Documented limitations / devices / risks | this README |

## Run and test

```shell
# from the repository root
node --test services/player-count/tests/*.test.mjs

# run the service locally (development only)
METALLIC_PLAYER_COUNT_INGEST_TOKEN=<strong-secret> \
METALLIC_PLAYER_COUNT_READ_TOKEN=<strong-secret> \
METALLIC_PLAYER_COUNT_ENV=production \
node services/player-count/src/main.mjs
```

Environment variables:

```text
METALLIC_PLAYER_COUNT_INGEST_TOKEN   required, >= 16 chars, room-server fleet only
METALLIC_PLAYER_COUNT_READ_TOKEN     required, >= 16 chars, status consumers only
METALLIC_PLAYER_COUNT_ENV            default "production" (e.g. staging for lab)
METALLIC_PLAYER_COUNT_HOST           default "127.0.0.1"
METALLIC_PLAYER_COUNT_PORT           default 4040
```

The Discord bot reads the aggregate with `METALLIC_STATUS_URL` and
`METALLIC_STATUS_BEARER` (see [discord-bot README](../discord-bot/README.md)).

## Supported devices and platforms

- **Service runtime:** Node.js 22+ on Windows, Linux or macOS. CI runs on
  `ubuntu-latest`. No native modules, no external dependencies.
- **Game clients that could be counted:** none yet. The aggregator is
  transport- and platform-agnostic; the future PC / VR / mobile client fleet
  reports through room-server heartbeats. It is *not* tied to any specific
  headset or OS, and no client is claimed as supported.
- **Deployment target (future):** behind the protected HTTPS edge
  (see [ARCHITECTURE §10](../../docs/ARCHITECTURE.md)); never exposed directly to
  game clients.

## Limitations, risks and disclosures

- **No real data.** No room server, account service or game client exists in this
  repository, so the service has **no live production count**. The numbers in
  docs are schema examples, not Metallic metrics.
- **In-memory only.** State is process-local and lost on restart. A real
  deployment needs a durable or replicated store (and horizontal sharing) so
  counts survive restarts and multiple aggregator instances.
- **String validation is not attestation.** Authenticity comes from the
  authenticated transport and service tokens, not from `source`/`schema` checks.
  Room servers must be trusted and the channel TLS-protected.
- **Clock trust.** Freshness, ordering and skew limits assume reasonably
  synchronized clocks. NTP drift on a room server can cause rejection.
- **Ceiling and abuse.** There is a hard room-capacity (`maxRooms`) and per-body
  size limit, but no per-room rate limiting or anomaly detection yet; a
  compromised ingest token could push noisy data.
- **Privacy.** The public aggregate deliberately contains only counts and a
  timestamp. Any future per-room breakdown must respect private/unlisted rooms.
- **De-dup policy.** One account counts once regardless of device count. This is
  a policy choice; "multiple devices count more than once" is intentionally not
  supported.
- **Not a substitute for load testing.** This service counts sessions; it does
  not measure capacity. See [QUALITY_AND_STRESS_TESTING.md](../../docs/QUALITY_AND_STRESS_TESTING.md).

## Not implemented

Persistence and migrations, horizontal scaling, per-room rate limiting, alerting,
TLS/edge termination, room-server heartbeat emitters, account-service
integration, per-region rollups, and any real deployment. Until those exist, the
Discord bot must keep reporting **status unavailable** rather than a fabricated
count.
