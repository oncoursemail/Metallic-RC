#!/usr/bin/env python3
"""Model hypothetical game-fleet capacity, NOT measured Metallic performance.

Only use operator-supplied assumptions. Never report output as real CCU or
deployable production capacity before running the real protocol load tests.
Standard-library only; no network access or cloud credentials required.
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any


def _number(value: Any, label: str, *, allow_zero: bool = False) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{label} must be a finite number")
    if not math.isfinite(value) or (value < 0 if allow_zero else value <= 0):
        raise ValueError(f"{label} must be {'non-negative' if allow_zero else 'positive'} and finite")
    return float(value)


def _integer(value: Any, label: str, *, allow_zero: bool = False) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError(f"{label} must be an integer")
    if value < 0 if allow_zero else value <= 0:
        raise ValueError(f"{label} must be {'non-negative' if allow_zero else 'positive'}")
    return value


def _ratio(value: Any, label: str, *, allow_one: bool = True) -> float:
    value = _number(value, label, allow_zero=True)
    if value > 1 or (not allow_one and value == 1):
        raise ValueError(f"{label} must be in [0, {'1' if allow_one else '1)'}]")
    return value


def estimate(scenario: dict[str, Any]) -> dict[str, Any]:
    """Return a conservative arithmetic model from explicit assumptions only.

    Sessions are rounded up independently per region. Each region reserves
    extra nodes for incident/rollout failure. Voice is simplified to total
    per-recipient media kbps, not modeled as all-to-all peer connections.
    """
    if not isinstance(scenario, dict):
        raise ValueError("Scenario must be a JSON object")
    settings = scenario.get("assumptions")
    regions = scenario.get("regions")
    if not isinstance(settings, dict) or not isinstance(regions, list) or not regions:
        raise ValueError("Supply an assumptions object and nonempty regions list")

    players_per_session = _integer(settings.get("players_per_session"), "players_per_session")
    session_cpu = _number(settings.get("cpu_cores_per_session"), "cpu_cores_per_session")
    session_ram = _number(settings.get("ram_gib_per_session"), "ram_gib_per_session")
    host_cpu = _number(settings.get("cpu_cores_per_host"), "cpu_cores_per_host")
    host_ram = _number(settings.get("ram_gib_per_host"), "ram_gib_per_host")
    reserved_cpu = _number(settings.get("reserved_cpu_cores_per_host", 0), "reserved_cpu_cores_per_host", allow_zero=True)
    reserved_ram = _number(settings.get("reserved_ram_gib_per_host", 0), "reserved_ram_gib_per_host", allow_zero=True)
    headroom = _ratio(settings.get("capacity_headroom_fraction", 0.30), "capacity_headroom_fraction", allow_one=False)
    spare_nodes = _integer(settings.get("spare_hosts_per_active_region", 1), "spare_hosts_per_active_region", allow_zero=True)
    gameplay_kbps = _number(settings.get("server_to_client_game_kbps"), "server_to_client_game_kbps", allow_zero=True)
    voice_kbps = _number(settings.get("server_to_client_voice_kbps", 0), "server_to_client_voice_kbps", allow_zero=True)
    overhead = _number(settings.get("egress_overhead_fraction", 0), "egress_overhead_fraction", allow_zero=True)
    avg_fraction = _ratio(settings.get("average_to_peak_ccu_fraction"), "average_to_peak_ccu_fraction")
    month_hours = _number(settings.get("hours_in_month", 730), "hours_in_month")
    price_raw = settings.get("egress_usd_per_gib")
    egress_price = None if price_raw is None else _number(price_raw, "egress_usd_per_gib", allow_zero=True)

    usable_cpu = (host_cpu - reserved_cpu) * (1 - headroom)
    usable_ram = (host_ram - reserved_ram) * (1 - headroom)
    if usable_cpu <= 0 or usable_ram <= 0:
        raise ValueError("host reservations/headroom leave no usable compute or RAM")

    sessions_per_host = min(math.floor(usable_cpu / session_cpu),
                            math.floor(usable_ram / session_ram))
    if sessions_per_host < 1:
        raise ValueError("one session cannot fit on a host within reserved headroom")

    results: list[dict[str, Any]] = []
    names: set[str] = set()
    total_peak_players = 0
    total_sessions = 0
    total_hosts = 0
    for region in regions:
        if not isinstance(region, dict):
            raise ValueError("each region must be an object")
        name = region.get("name")
        if not isinstance(name, str) or not name.strip() or name in names:
            raise ValueError("region names must be unique nonempty strings")
        names.add(name)
        players = _integer(region.get("peak_ccu"), f"regions[{name}].peak_ccu", allow_zero=True)
        sessions = math.ceil(players / players_per_session)
        hosts_for_sessions = math.ceil(sessions / sessions_per_host)
        total_hosts_region = hosts_for_sessions + (spare_nodes if players else 0)
        total_peak_players += players
        total_sessions += sessions
        total_hosts += total_hosts_region
        results.append({
            "region": name,
            "peak_ccu_assumed": players,
            "required_sessions": sessions,
            "active_hosts": hosts_for_sessions,
            "spare_hosts": spare_nodes if players else 0,
            "estimated_hosts_total": total_hosts_region,
        })

    per_player_kbps = (gameplay_kbps + voice_kbps) * (1 + overhead)
    peak_egress_mbps = total_peak_players * per_player_kbps / 1000
    average_egress_mbps = peak_egress_mbps * avg_fraction
    monthly_bytes = average_egress_mbps * 1_000_000 / 8 * month_hours * 3600
    monthly_gib = monthly_bytes / (1024 ** 3)

    return {
        "result_type": "HYPOTHETICAL_CAPACITY_MODEL_NOT_MEASURED",
        "warning": "Do not report these figures as actual server capacity, player counts or production pricing.",
        "assumed_peak_ccu": total_peak_players,
        "assumed_average_ccu": round(total_peak_players * avg_fraction, 2),
        "players_per_session": players_per_session,
        "sessions_per_host_model": sessions_per_host,
        "total_sessions": total_sessions,
        "total_hosts_including_regional_spares": total_hosts,
        "model_peak_egress_mbps": round(peak_egress_mbps, 3),
        "model_average_egress_mbps": round(average_egress_mbps, 3),
        "model_monthly_egress_gib": round(monthly_gib, 3),
        "model_monthly_egress_cost_usd": (
            round(monthly_gib * egress_price, 2) if egress_price is not None else None
        ),
        "cost_scope": "Egress only. Excludes servers, CDN downloads, databases, voice compute, staffing, DDoS products, backups and taxes.",
        "per_region": results,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scenario_json", type=Path, help="Path to JSON assumptions")
    args = parser.parse_args(argv)
    try:
        scenario = json.loads(args.scenario_json.read_text(encoding="utf-8"))
        result = estimate(scenario)
    except (ValueError, OSError, json.JSONDecodeError) as exc:
        print(f"Capacity modeling failed: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
