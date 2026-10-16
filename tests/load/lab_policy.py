#!/usr/bin/env python3
"""Rate/resource caps, scenario manifests, error budgets and evidence labels.

This module governs the **LOCAL synthetic mock lab** only. Nothing here
describes or measures real Metallic game-server capacity, real players online,
the shipping client wire protocol, rendering/VR performance or any production
service. Every artifact it produces is explicitly labelled synthetic.
"""
from __future__ import annotations

import json
from pathlib import Path

LOCAL_HOSTS = ("127.0.0.1", "::1", "localhost")
SCENARIO_DIR = Path(__file__).resolve().parent / "scenarios"
MODES = ("explore", "builder", "mixed", "idle", "reconnect")

# Hard upper bounds. These exist to make accidental runaway jobs impossible for
# this localhost fixture; they are NOT a statement about game capacity.
DEFAULT_CAPS = {
    "maxClients": 200,
    "maxActionsPerClient": 200,
    "maxSpawnRate": 100.0,
    "maxTotalActions": 20000,
    "maxDurationSeconds": 120.0,
    "maxLatencyP95Ms": 250.0,
    "maxErrorRatio": 0.05,
    "maxActionsPerSecond": 5000.0,
}

MODE_IDS = ("explore", "builder", "mixed", "idle", "reconnect")

EVIDENCE_LABELS = {
    "evidenceClass": "synthetic-local-mock",
    "testType": "local-mock-protocol-only",
    "notActualGamePlayers": True,
    "countTowardsPublicPlayersOnline": False,
    "provesRealGameCapacity": False,
    "measures": "mock fixture request handling on loopback only",
    "doesNotMeasure": [
        "real game wire protocol",
        "rendering, animation or VR/headset performance",
        "multi-region or public-internet networking",
        "real game-server or real player capacity",
    ],
}


class ScenarioError(ValueError):
    """Raised when a scenario manifest violates the lab policy."""


def merged_caps(caps: dict | None = None) -> dict:
    merged = dict(DEFAULT_CAPS)
    for key, value in (caps or {}).items():
        if key not in DEFAULT_CAPS:
            raise ScenarioError(f"unknown cap: {key}")
        merged[key] = value
    return merged


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ScenarioError(message)


def _number(value, field: str) -> float:
    _require(isinstance(value, (int, float)) and not isinstance(value, bool),
             f"{field} must be a number")
    return float(value)


def _integer(value, field: str) -> int:
    _require(isinstance(value, int) and not isinstance(value, bool),
             f"{field} must be an integer")
    return value


def validate_scenario(scenario: dict, caps: dict | None = None) -> dict:
    """Validate and normalise one scenario manifest against the lab caps."""
    _require(isinstance(scenario, dict), "scenario must be an object")
    limits = merged_caps(caps)
    _require(scenario.get("schema") == 1, "scenario schema must be 1")

    scenario_id = scenario.get("id")
    _require(isinstance(scenario_id, str) and scenario_id.strip() != "", "scenario id is required")
    _require(all(ch.isalnum() or ch in "-_" for ch in scenario_id),
             "scenario id may only contain letters, digits, '-' and '_'")

    mode = scenario.get("mode")
    _require(mode in MODES, f"unsupported scenario mode: {mode!r}")

    clients = _integer(scenario.get("clients"), "clients")
    _require(1 <= clients <= limits["maxClients"],
             f"clients must be between 1 and {int(limits['maxClients'])}")

    actions = _integer(scenario.get("actions"), "actions")
    _require(1 <= actions <= limits["maxActionsPerClient"],
             f"actions must be between 1 and {int(limits['maxActionsPerClient'])}")

    spawn_rate = _number(scenario.get("spawnRate"), "spawnRate")
    _require(0.1 <= spawn_rate <= limits["maxSpawnRate"],
             f"spawnRate must be between 0.1 and {limits['maxSpawnRate']}")

    seed = _integer(scenario.get("seed"), "seed")
    _require(seed >= 0, "seed must be >= 0")

    total_actions = clients * actions
    _require(total_actions <= limits["maxTotalActions"],
             f"total actions ({total_actions}) exceed cap {int(limits['maxTotalActions'])}")

    spawn_window = clients / spawn_rate if spawn_rate else 0.0
    _require(spawn_window <= limits["maxDurationSeconds"],
             f"spawn window {spawn_window:.1f}s exceeds duration cap "
             f"{limits['maxDurationSeconds']:.0f}s")

    raw_budget = scenario.get("budget", {})
    _require(isinstance(raw_budget, dict), "budget must be an object")
    budget = _normalise_budget(raw_budget, limits, total_actions)

    return {
        "schema": 1,
        "id": scenario_id,
        "title": str(scenario.get("title", scenario_id)),
        "description": str(scenario.get("description", "")),
        "mode": mode,
        "clients": clients,
        "actions": actions,
        "spawnRate": spawn_rate,
        "seed": seed,
        "budget": budget,
        "expectedTotalActions": total_actions,
        "caps": limits,
    }


def _normalise_budget(raw: dict, limits: dict, total_actions: int) -> dict:
    allowed = {
        "maxErrorRatio", "minSuccessfulRatio", "maxLatencyP95Ms", "minActionsSentRatio",
    }
    for key in raw:
        _require(key in allowed, f"unknown budget field: {key}")

    max_error = _number(raw.get("maxErrorRatio", 0.0), "budget.maxErrorRatio")
    _require(0.0 <= max_error <= limits["maxErrorRatio"],
             f"budget.maxErrorRatio must be between 0 and the cap {limits['maxErrorRatio']}")

    min_success = _number(raw.get("minSuccessfulRatio", 1.0), "budget.minSuccessfulRatio")
    _require(0.0 <= min_success <= 1.0, "budget.minSuccessfulRatio must be between 0 and 1")

    max_p95 = _number(raw.get("maxLatencyP95Ms", limits["maxLatencyP95Ms"]), "budget.maxLatencyP95Ms")
    _require(max_p95 <= limits["maxLatencyP95Ms"],
             f"budget.maxLatencyP95Ms must be <= the cap {limits['maxLatencyP95Ms']}")

    min_actions = _number(raw.get("minActionsSentRatio", 1.0), "budget.minActionsSentRatio")
    _require(0.0 <= min_actions <= 1.0, "budget.minActionsSentRatio must be between 0 and 1")

    return {
        "maxErrorRatio": max_error,
        "minSuccessfulRatio": min_success,
        "maxLatencyP95Ms": max_p95,
        "minActionsSentRatio": min_actions,
        "expectedTotalActions": total_actions,
    }


def load_scenario(path: str | Path) -> dict:
    path = Path(path)
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ScenarioError(f"could not read scenario {path}: {error}") from error
    return validate_scenario(document)


def load_scenarios(directory: str | Path = SCENARIO_DIR) -> dict[str, dict]:
    directory = Path(directory)
    _require(directory.is_dir(), f"scenario directory not found: {directory}")
    scenarios: dict[str, dict] = {}
    for path in sorted(directory.glob("*.json")):
        scenario = load_scenario(path)
        _require(scenario["id"] not in scenarios, f"duplicate scenario id: {scenario['id']}")
        scenarios[scenario["id"]] = scenario
    _require(bool(scenarios), f"no scenario manifests found in {directory}")
    return scenarios


def evaluate_budget(report: dict, budget: dict, caps: dict | None = None) -> dict:
    """Compare a client-side run report against an error/latency budget."""
    limits = merged_caps(caps)
    requested = float(report.get("requested") or 0)
    failed = float(report.get("failed") or 0)
    successful = float(report.get("successful") or 0)
    expected_actions = float(report.get("expectedActions") or 0)
    sent_actions = float(report.get("actionsSent") or 0)
    p95 = float(report.get("requestLatencyMs", {}).get("p95") or 0.0)

    error_ratio = (failed / requested) if requested else 1.0
    success_ratio = (successful / requested) if requested else 0.0
    actions_ratio = (sent_actions / expected_actions) if expected_actions else 1.0

    max_error = float(budget.get("maxErrorRatio", limits["maxErrorRatio"]))
    min_success = float(budget.get("minSuccessfulRatio", 1.0))
    max_p95 = float(budget.get("maxLatencyP95Ms", limits["maxLatencyP95Ms"]))
    min_actions = float(budget.get("minActionsSentRatio", 1.0))

    checks = [
        _check("errorRatio", error_ratio, "<=", max_error),
        _check("successRatio", success_ratio, ">=", min_success),
        _check("latencyP95Ms", p95, "<=", max_p95, unit="ms"),
        _check("actionsSentRatio", actions_ratio, ">=", min_actions),
    ]
    return {
        "passed": all(check["passed"] for check in checks),
        "failedChecks": [check["name"] for check in checks if not check["passed"]],
        "checks": checks,
    }


def _check(name: str, observed: float, comparison: str, limit: float, unit: str = "ratio") -> dict:
    passed = observed <= limit if comparison == "<=" else observed >= limit
    return {
        "name": name,
        "passed": passed,
        "observed": round(observed, 4),
        "comparison": comparison,
        "limit": limit,
        "unit": unit,
    }


def build_artifact(
    scenario: dict,
    report: dict,
    server_metrics: dict,
    verdict: dict,
    run_id: str,
    started_utc: str,
    environment: str = "localhost-loopback-mock",
) -> dict:
    """Assemble an immutable evidence document for one local run."""
    return {
        "schema": 1,
        "kind": "metallic-local-mock-lab-run",
        "runId": run_id,
        "startedUtc": started_utc,
        "environment": environment,
        "labels": dict(EVIDENCE_LABELS),
        "scenario": {
            "id": scenario["id"],
            "title": scenario["title"],
            "description": scenario["description"],
            "mode": scenario["mode"],
            "clients": scenario["clients"],
            "actions": scenario["actions"],
            "spawnRate": scenario["spawnRate"],
            "seed": scenario["seed"],
        },
        "caps": dict(scenario["caps"]),
        "budget": dict(scenario["budget"]),
        "clientReport": report,
        "serverMetrics": server_metrics,
        "verdict": verdict,
    }


def write_artifact(document: dict, out_dir: str | Path, run_id: str, overwrite: bool = False) -> Path:
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    _require(all(ch.isalnum() or ch in "-_." for ch in run_id), "unsafe run id")
    path = out_dir / f"{run_id}.json"
    if path.exists() and not overwrite:
        raise ScenarioError(f"refusing to overwrite existing artifact: {path}")
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def write_index(out_dir: str | Path, entries: list[dict], generated_utc: str) -> Path:
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    document = {
        "schema": 1,
        "kind": "metallic-local-mock-lab-index",
        "generatedUtc": generated_utc,
        "labels": dict(EVIDENCE_LABELS),
        "runs": entries,
    }
    path = out_dir / "index.json"
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path
