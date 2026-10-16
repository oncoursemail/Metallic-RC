#!/usr/bin/env python3
"""Deterministic synthetic actors against a LOCAL JSONL mock room.

This is a functional harness for early test automation, NOT a real game-server
stress result or a production player simulator. Real wire-protocol agents are a
separate development milestone. External hosts are intentionally blocked.
"""
from __future__ import annotations

import argparse
import asyncio
import json
import math
import random
import time


TEST_ROOM = "__metallic_local_synthetic_lab__"


async def rpc(reader: asyncio.StreamReader, writer: asyncio.StreamWriter, payload: dict) -> tuple[dict, float]:
    start = time.perf_counter()
    writer.write((json.dumps(payload, separators=(",", ":")) + "\n").encode())
    await writer.drain()
    line = await asyncio.wait_for(reader.readline(), timeout=5)
    if not line:
        raise ConnectionError("Fixture closed connection")
    message = json.loads(line)
    if not isinstance(message, dict):
        raise ValueError("Expected JSON object")
    return message, (time.perf_counter() - start) * 1000


async def actor_run(
    number: int,
    host: str,
    port: int,
    seed: int,
    actions: int,
    mode: str,
) -> dict:
    rnd = random.Random(seed ^ (number * 0x9E3779B1))
    actor = "testbot_" + str(seed) + "_" + str(number)
    times: list[float] = []
    errors: list[str] = []
    sent = 0
    joined = False
    writer = None
    try:
        reader, writer = await asyncio.wait_for(asyncio.open_connection(host, port), timeout=5)
        response, elapsed = await rpc(reader, writer, {
            "op": "join", "room": TEST_ROOM, "actor": actor, "synthetic": True
        })
        times.append(elapsed)
        if not response.get("ok") or response.get("synthetic") is not True:
            raise RuntimeError("Synthetic join rejected: " + str(response))
        joined = True
        for seq in range(actions):
            if mode == "builder":
                action = "interact" if rnd.random() < 0.8 else "move"
            elif mode == "explore":
                action = "move" if rnd.random() < 0.85 else "interact"
            else:
                action = "move" if seq % 2 == 0 else "interact"
            response, elapsed = await rpc(reader, writer, {
                "op": action, "seq": seq,
                "x": round(rnd.uniform(-50, 50), 3),
                "y": 0, "z": round(rnd.uniform(-50, 50), 3),
            })
            times.append(elapsed)
            sent += 1
            if not response.get("ok") or response.get("seq") != seq:
                raise RuntimeError("Fixture action acknowledgment mismatch")
            # Pacing avoids treating a tight CPU loop as typical human activity.
            await asyncio.sleep(rnd.uniform(0.001, 0.004))
        response, elapsed = await rpc(reader, writer, {"op": "leave"})
        times.append(elapsed)
        if not response.get("ok"):
            errors.append("leave_not_confirmed")
    except (OSError, ValueError, RuntimeError, asyncio.TimeoutError, ConnectionError) as error:
        errors.append(str(error)[:160])
    finally:
        if writer is not None:
            writer.close()
            try:
                await writer.wait_closed()
            except OSError:
                pass
    return {"actor": actor, "joined": joined, "sent": sent,
            "latenciesMs": times, "errors": errors, "synthetic": True}


def pctl_ms(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, math.ceil(p * len(ordered)) - 1))
    return round(ordered[index], 2)


async def run_scenario(host: str, port: int, clients: int, actions: int, seed: int,
                       spawn_rate: float, mode: str) -> dict:
    if host not in ("127.0.0.1", "::1", "localhost"):
        raise ValueError("This fixture runner only targets a local mock server")
    if not 1 <= clients <= 200:
        raise ValueError("clients must be between 1 and 200 for this local fixture")
    if not 1 <= actions <= 200:
        raise ValueError("actions must be between 1 and 200")
    if not 0.1 <= spawn_rate <= 100:
        raise ValueError("spawn rate must be between 0.1 and 100")
    if mode not in ("explore", "builder", "mixed"):
        raise ValueError("unsupported synthetic behavior mode")
    if not 1 <= port <= 65535:
        raise ValueError("port outside valid range")

    started = time.perf_counter()
    async def spawn(idx: int):
        await asyncio.sleep(idx / spawn_rate)
        return await actor_run(idx, host, port, seed, actions, mode)

    # Hard timeout bounds accidental infinite jobs. Spawn plus a limited working
    # period must fit within 120 seconds; this is a small lab tool, not a flooder.
    try:
        results = await asyncio.wait_for(
            asyncio.gather(*(spawn(i) for i in range(clients))),
            timeout=120,
        )
    except asyncio.TimeoutError:
        raise RuntimeError("Synthetic fixture run timed out") from None

    duration = max(.001, time.perf_counter() - started)
    latencies = [x for result in results for x in result["latenciesMs"]]
    errors = [error for result in results for error in result["errors"]]
    successful = sum(1 for result in results if result["joined"] and not result["errors"])
    actions_sent = sum(result["sent"] for result in results)
    return {
        "schema": 1,
        "testType": "local-mock-protocol-only",
        "notActualGamePlayers": True,
        "countTowardsPublicPlayersOnline": False,
        "mode": mode,
        "seed": seed,
        "requested": clients,
        "successful": successful,
        "failed": len(results) - successful,
        "actionsSent": actions_sent,
        "durationSeconds": round(duration, 3),
        "actionsPerSecond": round(actions_sent / duration, 1),
        "requestLatencyMs": {
            "p50": pctl_ms(latencies, .50),
            "p95": pctl_ms(latencies, .95),
            "p99": pctl_ms(latencies, .99),
        },
        "errors": errors[:20],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--clients", type=int, default=12)
    parser.add_argument("--actions", type=int, default=12)
    parser.add_argument("--seed", type=int, default=2026)
    parser.add_argument("--spawn-rate", type=float, default=12)
    parser.add_argument("--mode", choices=["explore", "builder", "mixed"], default="mixed")
    options = parser.parse_args()
    try:
        report = asyncio.run(run_scenario(options.host, options.port, options.clients,
                                         options.actions, options.seed, options.spawn_rate, options.mode))
    except (ValueError, RuntimeError, ConnectionError) as error:
        parser.error(str(error))
        return 2
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if not report["failed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
