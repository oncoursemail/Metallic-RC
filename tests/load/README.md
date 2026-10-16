# Metallic synthetic staging lab (localhost mock)

Reusable, reproducible harness that turns the early mock-player tests into a
controlled **staging lab**: declarative scenario manifests, hard rate/resource
caps, error budgets and metric artifacts. It runs entirely on loopback.

> **Evidence class: `synthetic-local-mock`.** These runs prove that the test
> scaffolding, caps, budgets and fixtures work. They do **not** measure the real
> Metallic game protocol, rendering/VR performance, real network regions or
> real game-server/player capacity, and they never count toward public players
> online. No numbers from this lab may be reported as game capacity.

## Files

| Path | Purpose |
| --- | --- |
| `mock_room_server.py` | LOCAL JSONL mock room + server-side counters (`MockRoomMetrics`). |
| `simulated_players.py` | Deterministic seeded actors (`explore`, `builder`, `mixed`, `idle`, `reconnect`). |
| `lab_policy.py` | Rate/resource caps, manifest validation, error budgets, evidence labels, artifact writers. |
| `run_lab.py` | CLI that starts the fixture, replays manifests and writes artifacts. |
| `scenarios/*.json` | Declarative scenario manifests. |
| `../test_staging_lab.py` | Unit + end-to-end tests for all of the above. |

## Reproducible run

Requirements: Python 3.11+ (CI uses 3.12). No third-party packages.

```shell
# Run the controlled suite + policy unit tests
python -m unittest discover -s tests -p 'test_*.py' -v

# Replay every bundled manifest and write metric artifacts
python tests/load/run_lab.py --all --out artifacts/load-runs

# Replay one scenario on an ephemeral loopback port
python tests/load/run_lab.py --scenario reconnect-local --out artifacts/load-runs
```

The CLI exits `0` only when every scenario passes its error budget. Artifacts are
written to `artifacts/load-runs/<runId>.json` plus an `index.json` summary, and
are uploaded as the `metallic-load-lab-metrics` CI artifact. Re-running an
existing run id is refused unless `--overwrite` is given, so evidence documents
stay immutable by default.

## Scenario manifest schema

```json
{
  "schema": 1,
  "id": "smoke-local",
  "title": "Local mock smoke",
  "description": "Short human explanation.",
  "mode": "mixed",
  "clients": 6,
  "actions": 4,
  "spawnRate": 60,
  "seed": 31,
  "budget": {
    "maxErrorRatio": 0.0,
    "minSuccessfulRatio": 1.0,
    "maxLatencyP95Ms": 250,
    "minActionsSentRatio": 1.0
  }
}
```

- `mode` is one of `explore`, `builder`, `mixed`, `idle`, `reconnect`.
- `seed` makes every run deterministic and replayable.
- `budget` is evaluated per run; a run that breaches any check fails the suite.

## Rate and resource caps

All caps live in `lab_policy.DEFAULT_CAPS` and are enforced at manifest load time
(a manifest can only make them *tighter*, never looser):

| Cap | Default | Why |
| --- | ---: | --- |
| `maxClients` | 200 | bound concurrency on a single loopback host |
| `maxActionsPerClient` | 200 | bound per-actor work |
| `maxSpawnRate` | 100 /s | bound ramp speed |
| `maxTotalActions` | 20 000 | bound total fixture work |
| `maxDurationSeconds` | 120 | bound accidental long jobs |
| `maxLatencyP95Ms` | 250 | budget ceiling |
| `maxErrorRatio` | 0.05 | budget ceiling |
| `maxActionsPerSecond` | 5 000 | bound sustained request rate |

The runner **refuses any non-loopback host**, so the lab cannot be pointed at a
remote or third-party target.

## What one artifact contains

- `labels` — explicit "synthetic / not real players / not real capacity" flags.
- `scenario` + `caps` + `budget` — exactly what was requested.
- `clientReport` — requested/successful/failed actors, actions sent vs expected,
  reconnects, request latency p50/p95/p99.
- `serverMetrics` — fixture-side accepted connections, joins, rejections, leaves,
  actions, malformed messages and peak concurrent synthetic sessions.
- `verdict` — per-check pass/fail, failed check names and a ghost-session check.

## Limitations, supported devices and risks

**Limitations**
- The mock protocol is a small JSONL fixture, not the shipping game wire
  protocol, authoritative simulation or replication.
- Loopback only: results say nothing about real latency, jitter, packet loss,
  NAT traversal or multi-region routing.
- Single process / single host: no distributed load generation, so generator
  CPU is never separated from a real server.
- No renderer, animation, physics, VFX, voice/SFU, database, cache or economy
  behaviour is exercised. VR comfort and graphics cannot be judged here.
- Absolute latency numbers are meaningless outside the machine that produced them;
  only pass/fail against a declared budget is meaningful.

**Supported devices / hosts**
- Windows, Linux and macOS on x86-64/arm64 with Python 3.11+.
- CI: GitHub-hosted `ubuntu-latest`, Python 3.12.
- No GPU, headset, mobile device, network simulator or external service is used.

**Risks and mitigations**
- *Misuse as a flooder*: hard client/rate/duration caps and loopback-only host
  binding; non-loopback targets raise before any traffic.
- *False capacity claims*: every report and artifact carries explicit
  `notActualGamePlayers` / `provesRealGameCapacity: false` labels, and tests
  assert they are present.
- *Secret/account leakage*: the lab uses only in-process synthetic identities and
  requires no credentials or real accounts.
- *CI cost*: manifests are capped at tens of clients and sub-second runs; the
  workflow has a short timeout and uploads a small JSON artifact.
- *Flaky timing thresholds*: budgets use generous local ceilings (p95 ≤ 250 ms)
  and deterministic seeds; assertions target pass/fail and counts, not absolute
  hardware timings.
