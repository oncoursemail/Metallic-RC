#!/usr/bin/env python3
"""Reproducible LOCAL staging-lab runner for the synthetic mock fixture.

Starts the in-process mock room on loopback, replays one or more scenario
manifests, enforces rate/resource caps and error budgets, and writes metric
artifacts. This is test automation scaffolding, NOT a game load generator: it
cannot and does not measure real Metallic game-server capacity or real players.
"""
from __future__ import annotations

import argparse
import asyncio
import datetime
import json
import platform
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import lab_policy  # noqa: E402
from mock_room_server import MockRoomMetrics, start_mock_server  # noqa: E402
from simulated_players import run_scenario  # noqa: E402

DEFAULT_OUT = Path("artifacts") / "load-runs"


def utc_now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def run_id_for(scenario: dict, stamp: str) -> str:
    compact = stamp.replace("-", "").replace(":", "")
    return f"{scenario['id']}-{compact}-{scenario['seed']}"


async def run_selected(
    host: str,
    port: int,
    scenario_dir: str | Path,
    ids: list[str] | None,
    out_dir: str | Path,
    overwrite: bool = False,
) -> dict:
    if host not in lab_policy.LOCAL_HOSTS:
        raise ValueError("The synthetic lab only targets loopback hosts")

    scenarios = lab_policy.load_scenarios(scenario_dir)
    if ids:
        missing = [name for name in ids if name not in scenarios]
        if missing:
            raise ValueError("unknown scenario id(s): " + ", ".join(missing))
        scenarios = {name: scenarios[name] for name in ids}

    metrics = MockRoomMetrics()
    server, players = await start_mock_server(host, port, metrics)
    bound_port = server.sockets[0].getsockname()[1]
    out_dir = Path(out_dir)
    stamp = utc_now()
    entries: list[dict] = []
    try:
        for name, scenario in scenarios.items():
            metrics.reset()
            report = await run_scenario(
                host, bound_port,
                clients=scenario["clients"],
                actions=scenario["actions"],
                seed=scenario["seed"],
                spawn_rate=scenario["spawnRate"],
                mode=scenario["mode"],
            )
            # Let the fixture finish asynchronous disconnect cleanup before the
            # final ghost-session assertion, mirroring a short drain window.
            for _ in range(50):
                if not players:
                    break
                await asyncio.sleep(0.01)
            server_metrics = metrics.snapshot()
            verdict = lab_policy.evaluate_budget(report, scenario["budget"], scenario["caps"])
            verdict["ghostSessionsAfterRun"] = len(players)
            verdict["noGhostSessions"] = len(players) == 0
            verdict["passed"] = verdict["passed"] and verdict["noGhostSessions"]

            artifact = lab_policy.build_artifact(
                scenario, report, server_metrics, verdict,
                run_id=run_id_for(scenario, stamp),
                started_utc=stamp,
            )
            artifact["runtime"] = {
                "python": platform.python_version(),
                "platform": platform.system(),
                "host": host,
                "port": bound_port,
            }
            path = lab_policy.write_artifact(
                artifact, out_dir, artifact["runId"], overwrite=overwrite
            )
            entries.append({
                "runId": artifact["runId"],
                "scenario": name,
                "passed": verdict["passed"],
                "failedChecks": verdict["failedChecks"],
                "clientReport": report,
                "serverMetrics": server_metrics,
                "artifact": path.name,
            })
    finally:
        server.close()
        await server.wait_closed()

    index_path = lab_policy.write_index(out_dir, entries, stamp)
    return {
        "schema": 1,
        "kind": "metallic-local-mock-lab-summary",
        "generatedUtc": stamp,
        "labels": dict(lab_policy.EVIDENCE_LABELS),
        "scenarioDirectory": str(scenario_dir),
        "artifactDirectory": str(out_dir),
        "index": index_path.name,
        "allPassed": all(entry["passed"] for entry in entries),
        "runs": entries,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario-dir", default=str(lab_policy.SCENARIO_DIR))
    parser.add_argument("--scenario", action="append", default=None,
                        help="scenario id to run (repeatable); default runs every manifest")
    parser.add_argument("--out", default=str(DEFAULT_OUT), help="artifact output directory")
    parser.add_argument("--host", default="127.0.0.1", help="loopback host only")
    parser.add_argument("--port", type=int, default=0, help="0 chooses an ephemeral port")
    parser.add_argument("--overwrite", action="store_true",
                        help="allow replacing an artifact with the same run id")
    options = parser.parse_args()

    try:
        summary = asyncio.run(run_selected(
            options.host, options.port, options.scenario_dir,
            options.scenario, options.out, overwrite=options.overwrite,
        ))
    except (ValueError, lab_policy.ScenarioError, RuntimeError) as error:
        parser.error(str(error))
        return 2

    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if summary["allPassed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
