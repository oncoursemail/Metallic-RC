"""End-to-end self-contained tests for the fake-player LOCAL fixture, not game load."""
import asyncio
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tests" / "load"))
from mock_room_server import start_mock_server
from simulated_players import run_scenario, rpc


class SimulatedPlayerTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.server, self.players = await start_mock_server()
        self.port = self.server.sockets[0].getsockname()[1]

    async def asyncTearDown(self):
        self.server.close()
        await self.server.wait_closed()

    async def test_twelve_agents_make_real_mock_protocol_requests(self):
        result = await run_scenario("127.0.0.1", self.port, clients=12, actions=4,
                                    seed=31, spawn_rate=80, mode="explore")
        self.assertEqual(result["requested"], 12)
        self.assertEqual(result["successful"], 12)
        self.assertEqual(result["failed"], 0)
        self.assertEqual(result["actionsSent"], 48)
        self.assertTrue(result["notActualGamePlayers"])
        self.assertFalse(result["countTowardsPublicPlayersOnline"])
        self.assertGreaterEqual(result["requestLatencyMs"]["p95"], 0)
        await asyncio.sleep(.02)
        self.assertEqual(self.players, set())

    async def test_reject_non_synthetic_join(self):
        reader, writer = await asyncio.open_connection("127.0.0.1", self.port)
        message, _ = await rpc(reader, writer, {
            "op": "join", "room": "__metallic_local_synthetic_lab__",
            "actor": "testbot_fake", "synthetic": False,
        })
        self.assertEqual(message.get("error"), "test_only_identity_required")
        writer.close()
        await writer.wait_closed()

    async def test_builder_actions(self):
        report = await run_scenario("127.0.0.1", self.port, clients=2, actions=5,
                                    seed=44, spawn_rate=50, mode="builder")
        self.assertEqual(report["actionsSent"], 10)
        self.assertEqual(report["failed"], 0)

    async def test_remote_target_disallowed(self):
        with self.assertRaises(ValueError):
            await run_scenario("203.0.113.99", self.port, clients=2, actions=4,
                               seed=1, spawn_rate=1, mode="mixed")

    async def test_bot_count_cap(self):
        with self.assertRaises(ValueError):
            await run_scenario("127.0.0.1", self.port, clients=201, actions=4,
                               seed=1, spawn_rate=1, mode="mixed")


if __name__ == "__main__":
    unittest.main()
