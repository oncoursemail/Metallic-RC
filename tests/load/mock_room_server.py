#!/usr/bin/env python3
"""LOCAL-only JSONL mock-room service for testing a synthetic player harness.

Not Metallic's game protocol, game server, production player-count API, or
authoritative multiplayer implementation. Never expose this test fixture.
"""
from __future__ import annotations

import asyncio
import json
import sys

MAX_LINE = 4096
ROOM = "__metallic_local_synthetic_lab__"


class MockRoomMetrics:
    """Server-side counters for the LOCAL fixture only.

    These numbers describe how the mock fixture answered synthetic loopback
    traffic. They are NOT game telemetry, NOT a real player count and NOT a
    measurement of any shipping Metallic service.
    """

    def __init__(self) -> None:
        self.reset()

    def reset(self) -> None:
        self.connections_accepted = 0
        self.successful_joins = 0
        self.join_rejections = 0
        self.leaves_confirmed = 0
        self.actions_accepted = 0
        self.malformed_messages = 0
        self.peak_concurrent_synthetic_sessions = 0

    def snapshot(self) -> dict[str, int]:
        return {
            "connectionsAccepted": self.connections_accepted,
            "successfulJoins": self.successful_joins,
            "joinRejections": self.join_rejections,
            "leavesConfirmed": self.leaves_confirmed,
            "actionsAccepted": self.actions_accepted,
            "malformedMessages": self.malformed_messages,
            "peakConcurrentSyntheticSessions": self.peak_concurrent_synthetic_sessions,
        }


async def handle_client(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    players: set[str],
    metrics: MockRoomMetrics | None = None,
) -> None:
    actor: str | None = None
    if metrics is not None:
        metrics.connections_accepted += 1
    try:
        while True:
            line = await asyncio.wait_for(reader.readline(), timeout=20)
            if not line:
                break
            if len(line) >= MAX_LINE:
                break
            try:
                message = json.loads(line)
                if not isinstance(message, dict):
                    raise ValueError("message is not an object")
                op = message.get("op")
                if op == "join":
                    if actor:
                        reply = {"ok": False, "error": "already_joined"}
                    elif message.get("room") != ROOM or message.get("synthetic") is not True:
                        reply = {"ok": False, "error": "test_only_identity_required"}
                    else:
                        candidate = message.get("actor")
                        if not isinstance(candidate, str) or not candidate.startswith("testbot_") or len(candidate) > 80:
                            reply = {"ok": False, "error": "invalid_test_actor"}
                        elif candidate in players:
                            reply = {"ok": False, "error": "duplicate_actor"}
                        else:
                            actor = candidate
                            players.add(actor)
                            if metrics is not None:
                                metrics.successful_joins += 1
                                metrics.peak_concurrent_synthetic_sessions = max(
                                    metrics.peak_concurrent_synthetic_sessions, len(players)
                                )
                            reply = {"ok": True, "op": "joined", "actor": actor, "synthetic": True}
                    if not reply.get("ok") and metrics is not None:
                        metrics.join_rejections += 1
                elif not actor:
                    reply = {"ok": False, "error": "join_required"}
                elif op in ("move", "interact"):
                    seq = message.get("seq")
                    if not isinstance(seq, int) or not 0 <= seq <= 100000:
                        reply = {"ok": False, "error": "bad_sequence"}
                    else:
                        if metrics is not None:
                            metrics.actions_accepted += 1
                        reply = {"ok": True, "op": "ack", "seq": seq, "actor": actor}
                elif op == "leave":
                    if metrics is not None:
                        metrics.leaves_confirmed += 1
                    reply = {"ok": True, "op": "left", "actor": actor}
                else:
                    reply = {"ok": False, "error": "unknown_operation"}
            except (json.JSONDecodeError, ValueError):
                if metrics is not None:
                    metrics.malformed_messages += 1
                reply = {"ok": False, "error": "malformed_message"}
            writer.write((json.dumps(reply, separators=(",", ":")) + "\n").encode())
            await writer.drain()
            if reply.get("op") == "left":
                break
    except (asyncio.TimeoutError, ConnectionResetError, BrokenPipeError):
        pass
    finally:
        if actor:
            players.discard(actor)
        writer.close()
        try:
            await writer.wait_closed()
        except (OSError, ConnectionError):
            pass


async def start_mock_server(
    host: str = "127.0.0.1",
    port: int = 0,
    metrics: MockRoomMetrics | None = None,
) -> tuple[asyncio.Server, set[str]]:
    if host not in ("127.0.0.1", "::1", "localhost"):
        raise ValueError("Fixture server must bind only to loopback")
    players: set[str] = set()
    server = await asyncio.start_server(lambda r, w: handle_client(r, w, players, metrics), host, port)
    return server, players


async def serve() -> None:
    server, _players = await start_mock_server(port=8765)
    addr = server.sockets[0].getsockname()
    print(f"Metallic mock room fixture listening only on {addr} (NOT a real game server)", flush=True)
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(serve())
    except KeyboardInterrupt:
        sys.exit(0)
