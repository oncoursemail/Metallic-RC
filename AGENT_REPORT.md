# AGENT_REPORT — Issue #36

**Issue:** [#36](https://github.com/oncoursemail/Metallic/issues/36) `[P0] Turn synthetic mock-player tests into a reusable staging lab`
**Branch:** `agent/deepseek-36` (committed only here; not pushed, `main` untouched)
**Date:** 2026-10-15

## Objective

Document a reproducible run, add scenario manifests, rate/resource caps, error
budgets and metric artifacts, while keeping the lab default **localhost-only**.

## Changes

New reusable localhost staging lab built on the existing `tests/load/` fixture:

| File | Change |
| --- | --- |
| `tests/load/lab_policy.py` | **New.** Rate/resource caps (`DEFAULT_CAPS`), manifest validation, error-budget evaluation, evidence labels and immutable artifact/index writers. |
| `tests/load/run_lab.py` | **New.** CLI runner: starts the loopback fixture, replays manifests, enforces caps/budgets, writes `artifacts/load-runs/<runId>.json` + `index.json`. Refuses non-loopback hosts. |
| `tests/load/scenarios/*.json` | **New.** Four declarative manifests: `smoke-local`, `explorer-local`, `builder-local`, `reconnect-local`. |
| `tests/load/mock_room_server.py` | Added `MockRoomMetrics` server-side counters (connections, joins, rejections, leaves, actions, malformed, peak sessions); backward-compatible signature. |
| `tests/load/simulated_players.py` | Added `idle` and `reconnect` modes; `reconnect` deliberately drops mid-run and rejoins to exercise ghost cleanup. Reports `expectedActions`/`reconnects`; explicit `provesRealGameCapacity: false`. |
| `tests/test_staging_lab.py` | **New.** 16 tests: cap enforcement, budget pass/fail, immutable labelled artifacts, controlled end-to-end suite, reconnect rejoin/cleanup, loopback-only enforcement. |
| `tests/load/README.md` | **New.** Reproducible run steps, manifest schema, cap table, artifact contents, and full limitations / supported devices / risks. |
| `.github/workflows/load-lab.yml` | **New.** CI job runs the controlled mock suite and uploads `metallic-load-lab-metrics`. |
| `.gitignore` | **New.** Ignore `artifacts/`, caches, `node_modules/`. |
| `docs/QUALITY_AND_STRESS_TESTING.md`, `README.md` | Document the implemented lab and link it. |

## Test output

```
python -m unittest discover -s tests -p 'test_*.py' -v
...
Ran 35 tests in 3.128s
OK
EXITCODE=0
```

Includes the 16 new `test_staging_lab.py` tests plus all pre-existing
`test_load_harness`, `test_progress`, `test_progress_svg` and `test_site` tests
(no regressions). `python -m compileall tests scripts services` and
`node --check docs/site/app.js` are clean.

Reproducible lab run (exit code 0 = every scenario within budget):

```
python tests/load/run_lab.py --all --out artifacts/load-runs
{
  "allPassed": true,
  "runs": [
    {"scenario": "builder-local",   "passed": true, "failedChecks": []},
    {"scenario": "explorer-local",  "passed": true, "failedChecks": []},
    {"scenario": "reconnect-local", "passed": true, "failedChecks": [], "reconnects": 6},
    {"scenario": "smoke-local",     "passed": true, "failedChecks": []}
  ]
}
```

Each artifact carries `labels.testType = "local-mock-protocol-only"`,
`labels.notActualGamePlayers = true`,
`labels.countTowardsPublicPlayersOnline = false`,
`labels.provesRealGameCapacity = false`, client-side latency percentiles and
fixture-side counters.

## Acceptance mapping

- **Controlled mock suite passes, evidence is distinct from real game capacity** —
  `run_lab.py --all` and `test_staging_lab.py` pass; every report/artifact is
  explicitly labelled synthetic with `provesRealGameCapacity: false` (asserted by
  tests).
- **Reproducible code/build/test evidence + passing CI** —
  deterministic seeds, documented commands in `tests/load/README.md`, and
  `.github/workflows/load-lab.yml` runs the suite and uploads metric artifacts.
- **Limitations, supported devices and risks documented** —
  `tests/load/README.md` "Limitations, supported devices and risks" section.

## Limitations

- Loopback-only JSONL mock protocol; it is **not** the shipping game wire
  protocol, authoritative simulation or replication.
- No rendering/VR, animation, physics, VFX, voice/SFU, database, cache or economy
  coverage; absolute latencies are machine-specific.
- Single host/process: no distributed load generation, so generator CPU is not
  separated from a server.
- Only the scaffolding, caps, budgets and fixtures are proven — **not** real
  game-server or real player capacity.

## Risks and mitigations

- Misuse as a flooder → hard caps plus loopback-only host binding; non-loopback
  targets raise before any traffic.
- False capacity claims → mandatory synthetic labels asserted by tests.
- Credential looming → no credentials or real accounts; in-process synthetic IDs.
- CI cost/flakiness → sub-second, tens-of-clients manifests, short timeout,
  deterministic seeds, generous local budgets.
