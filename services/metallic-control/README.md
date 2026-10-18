# Metallic Atlas — Custom Linux Server Control-Plane Lab

**Status: running, testable source code; not a production deployment or a game session server.**

This Go service and Linux agent are the first self-hostable components of Metallic's own backend. There are no external Go dependencies. The controller starts **loopback-only**, refuses all room admission until it receives recent authenticated node metrics and operator-configured resource budgets, and never starts or stops any process on your machines.

## What actually works

- **Node registration:** operator-owned JSON inventory. Agents never set their own capacity, region or permissions.
- **Node heartbeats:** authenticated per-node tokens; agent reads Linux `/proc/meminfo`, `/proc/loadavg`, and root filesystem free space without root.
- **Admission control:** refuse nodes that are stale, draining, high CPU load, low memory, high swap usage, or below minimum disk free space.
- **Scheduler:** reserve CPU milli-cores, MiB RAM and maximum rooms; tie-break on fewer reserved rooms, never overbook the declared budgets in concurrent requests.
- **Reservations:** TTL, idempotency/replay guards, one active reservation per room, renewal and release, bounded request size.
- **Single-process durability:** file-backed atomic reservation snapshots (restrictive permissions), restored on restart. Corrupt snapshots refuse startup rather than silently resetting.
- **Security basics:** constant-time token comparison, distinct operator vs node permissions, strict JSON, no CORS, no secrets returned or logged, bounded HTTP request sizes, loopback-only controller.
- **Tests:** unit, HTTP and race tests; GitHub CI confirms code builds and tests pass.

**Explicit limitations:** This does **not** authenticate real game players, create Unity/Godot/Rust game processes, monitor actual containers, provision cloud servers, execute jobs, support high availability, allocate network ports, stream voice, detect cheating, or prove any concurrent-player count. It has one in-process controller writer only; no cross-region quorum/failover. It is not safe to expose directly to the public internet. It is not a launcher or in-game API. Operators must not interpret `reservation` as a started game session.

## Layout

- `cmd/control`: loopback-only HTTP server; static inventory + state file.
- `cmd/agent`: independent unprivileged, Linux-only read-only host metrics heartbeat.
- `internal/control`: scheduler, strict HTTP contract and single-writer persistence.
- `ops/metallic-control/inventory.example.json`: fictional example. Do not mistake these values for validated capacity on owned machines.
- `.github/workflows/metallic-control-ci.yml`: lint/test/build gate.

## Run locally for an isolated lab

Requires Go 1.22+ installed on your **development** machine, not on the production servers. No Docker, root or database needed for this first lab.

```bash
cd services/metallic-control
go test -race ./...
go vet ./...

# Generate new secrets; never commit the output and never reuse these in production.
export METALLIC_ADMIN_TOKEN="$(openssl rand -hex 32)"
export METALLIC_NODE_TOKEN="$(openssl rand -hex 32)"
export METALLIC_AGENT_TOKENS_JSON="$(python3 -c 'import json,os;print(json.dumps({"lab-node-a":os.environ["METALLIC_NODE_TOKEN"]}))')"
export METALLIC_INVENTORY_PATH="$(pwd)/../../ops/metallic-control/inventory.example.json"
export METALLIC_STATE_PATH="$(mktemp -d)/reservations.json"
export METALLIC_LISTEN="127.0.0.1:8098"
go run ./cmd/control
```

In a second local shell (with the same node token exported from the first shell):

```bash
export METALLIC_NODE_ID=lab-node-a
export METALLIC_API_ORIGIN=http://127.0.0.1:8098
go run ./cmd/agent
```

The example's inactive `lab-node-b` remains draining. Your host could also be rejected by disk/swap/memory guards. **That is correct:** the scheduler fails closed under pressure. No game process will launch.

```bash
curl -fsS -H "Authorization: Bearer $METALLIC_ADMIN_TOKEN" http://127.0.0.1:8098/v1/nodes
curl -fsS -H "Authorization: Bearer $METALLIC_ADMIN_TOKEN" http://127.0.0.1:8098/v1/metrics
```

To *reserve* only (not start a server), after a healthy heartbeat:

```bash
curl -fsS -X POST http://127.0.0.1:8098/v1/reservations \
  -H "Authorization: Bearer $METALLIC_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: local-lab-room-00001' \
  -d '{"roomId":"demo-room-1","region":"lab-only","cpuMilli":500,"memoryMiB":256,"ttlSeconds":120}'
```

Use `POST /v1/reservations/{id}/renew` with `{ "ttlSeconds": 120 }` to extend; use `POST /v1/reservations/{id}/release` to release. All calls require administrator token. Never expose administrator bearer tokens to an untrusted game client or a website.

## API contract

| Verb | Endpoint | Token | Result |
|---|---|---|---|
| GET | `/healthz` | none | HTTP liveness, *not* gameplay readiness |
| POST | `/internal/nodes/{nodeID}/heartbeat` | that node's individual agent token | 204 or strict validation error |
| GET | `/v1/nodes` | admin | node states and last observed memory/load/disk |
| GET | `/v1/metrics` | admin | capacity/control-plane counters (not Prometheus yet) |
| GET | `/v1/reservations` | admin | active room reservations |
| POST | `/v1/reservations` | admin | 201 created, 200 idempotent replay; never launches a session |
| POST | `/v1/reservations/{id}/renew` | admin | renew TTL |
| POST | `/v1/reservations/{id}/release` | admin | stop reserving capacity (does not terminate a game process) |

Errors have a non-sensitive `error` code. No API is exposed for terminating/reconfiguring arbitrary processes, mounting volumes or running user-supplied shell commands.

## Production hardening checklist (not implemented)

1. Operator identity/OIDC+RBAC, per-agent mTLS and automatic key rotation with secrets service, proxy audit and strict allowed network paths.
2. PostgreSQL leader-election/fencing and durable transactional reservations, not file state; reliable scheduler/runner reconciliation.
3. Real instance runner adapter with immutable signed game-server images, exact resource limits, process supervision, kill switch, port management and health probes. Run rootless where feasible.
4. Region/capacity allocator, protected UDP endpoints and game join-ticket service; player API segregated from admin API.
5. Prometheus/OpenTelemetry metrics, Linux PSI/cgroup accounting, NUMA/CPU pinning experiment, thermal/network monitors, measured placement headroom.
6. Region-level failover, backups and tested restoration, chaos drills and CI deployment gates with real-player vs synthetic telemetry.
7. Threat modeling, fuzzing, security review, privacy and jurisdiction compliance, measured actual load, on-call and capacity budget.

**Read next:** [Hardware-aware design/research](../../docs/METALLIC_OWN_INFRASTRUCTURE_2026.md), [Architecture](../../docs/ARCHITECTURE.md), [load testing](../../docs/QUALITY_AND_STRESS_TESTING.md). Earlier global architecture research remains in [PR #65](https://github.com/oncoursemail/Metallic/pull/65) until merged.
