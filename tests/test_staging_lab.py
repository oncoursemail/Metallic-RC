"""Tests for the reusable LOCAL synthetic staging lab (issue #36).

Everything here exercises the localhost mock fixture and its policy layer. It
does not and cannot measure real game-server capacity or real players online.
"""
import asyncio
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
LOAD = ROOT / "tests" / "load"
sys.path.insert(0, str(LOAD))

import lab_policy  # noqa: E402
import run_lab  # noqa: E402
from mock_room_server import MockRoomMetrics, start_mock_server  # noqa: E402
from simulated_players import run_scenario  # noqa: E402


def sample_report(requested=10, successful=10, failed=0, expected=40, sent=40,
                  p95=5.0, errors=None):
    return {
        "requested": requested,
        "successful": successful,
        "failed": failed,
        "expectedActions": expected,
        "actionsSent": sent,
        "requestLatencyMs": {"p50": 2.0, "p95": p95, "p99": 9.0},
        "errors": list(errors or []),
    }


def valid_manifest(**overrides):
    manifest = {
        "schema": 1,
        "id": "unit-scenario",
        "title": "Unit",
        "mode": "mixed",
        "clients": 4,
        "actions": 3,
        "spawnRate": 40,
        "seed": 7,
        "budget": {"maxErrorRatio": 0.0, "minSuccessfulRatio": 1.0},
    }
    manifest.update(overrides)
    return manifest


class ScenarioPolicyTests(unittest.TestCase):
    def test_default_caps_bind_to_loopback_hosts(self):
        self.assertEqual(lab_policy.LOCAL_HOSTS, ("127.0.0.1", "::1", "localhost"))
        self.assertLessEqual(lab_policy.DEFAULT_CAPS["maxClients"], 200)
        self.assertTrue(lab_policy.EVIDENCE_LABELS["provesRealGameCapacity"] is False)

    def test_bundled_manifests_are_valid_and_unique(self):
        scenarios = lab_policy.load_scenarios()
        self.assertGreaterEqual(len(scenarios), 4)
        self.assertEqual(len(scenarios), len(set(scenarios)))
        for scenario in scenarios.values():
            self.assertEqual(scenario["schema"], 1)
            self.assertIn(scenario["mode"], lab_policy.MODES)

    def test_rejects_clients_over_cap(self):
        with self.assertRaises(lab_policy.ScenarioError):
            lab_policy.validate_scenario(valid_manifest(clients=201))

    def test_rejects_total_actions_over_cap(self):
        with self.assertRaises(lab_policy.ScenarioError):
            lab_policy.validate_scenario(valid_manifest(clients=200, actions=200))

    def test_rejects_bad_mode_and_bad_id(self):
        with self.assertRaises(lab_policy.ScenarioError):
            lab_policy.validate_scenario(valid_manifest(mode="marketplace"))
        with self.assertRaises(lab_policy.ScenarioError):
            lab_policy.validate_scenario(valid_manifest(id="bad id!"))

    def test_rejects_unknown_budget_field_and_loose_budget(self):
        with self.assertRaises(lab_policy.ScenarioError):
            lab_policy.validate_scenario(valid_manifest(budget={"maxPlayers": 5}))
        with self.assertRaises(lab_policy.ScenarioError):
            lab_policy.validate_scenario(valid_manifest(budget={"maxErrorRatio": 5.0}))

    def test_caps_can_be_tightened_but_not_widened_unknown(self):
        with self.assertRaises(lab_policy.ScenarioError):
            lab_policy.merged_caps({"maxClientsExtra": 1})
        self.assertEqual(lab_policy.merged_caps({"maxClients": 10})["maxClients"], 10)


class ErrorBudgetTests(unittest.TestCase):
    def test_passing_report_passes_all_checks(self):
        verdict = lab_policy.evaluate_budget(
            sample_report(), {"maxErrorRatio": 0.0, "minSuccessfulRatio": 1.0})
        self.assertTrue(verdict["passed"])
        self.assertEqual(verdict["failedChecks"], [])

    def test_error_ratio_breach_fails(self):
        verdict = lab_policy.evaluate_budget(
            sample_report(successful=8, failed=2), {"maxErrorRatio": 0.0})
        self.assertFalse(verdict["passed"])
        self.assertIn("errorRatio", verdict["failedChecks"])

    def test_latency_and_action_breaches_fail(self):
        verdict = lab_policy.evaluate_budget(
            sample_report(p95=999.0, sent=20),
            {"maxLatencyP95Ms": 250, "minActionsSentRatio": 1.0})
        self.assertFalse(verdict["passed"])
        self.assertIn("latencyP95Ms", verdict["failedChecks"])
        self.assertIn("actionsSentRatio", verdict["failedChecks"])


class ArtifactTests(unittest.TestCase):
    def test_artifact_is_labelled_synthetic_and_write_is_immutable(self):
        scenario = lab_policy.validate_scenario(valid_manifest())
        report = sample_report()
        verdict = lab_policy.evaluate_budget(report, scenario["budget"])
        artifact = lab_policy.build_artifact(
            scenario, report, {"successfulJoins": 4}, verdict,
            run_id="unit-scenario-1", started_utc="2026-10-15T00:00:00Z")
        with tempfile.TemporaryDirectory() as tmp:
            path = lab_policy.write_artifact(artifact, tmp, artifact["runId"])
            self.assertTrue(path.exists())
            self.assertFalse(artifact["labels"]["countTowardsPublicPlayersOnline"])
            self.assertEqual(artifact["labels"]["testType"], "local-mock-protocol-only")
            with self.assertRaises(lab_policy.ScenarioError):
                lab_policy.write_artifact(artifact, tmp, artifact["runId"])

    def test_index_records_all_runs(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = lab_policy.write_index(tmp, [{"runId": "a", "passed": True}],
                                         "2026-10-15T00:00:00Z")
            document = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(document["runs"][0]["runId"], "a")
            self.assertFalse(document["labels"]["provesRealGameCapacity"])


class StagingLabSuiteTests(unittest.IsolatedAsyncioTestCase):
    async def test_controlled_mock_suite_passes_and_writes_artifacts(self):
        scenarios = lab_policy.load_scenarios()
        with tempfile.TemporaryDirectory() as tmp:
            summary = await run_lab.run_selected(
                "127.0.0.1", 0, lab_policy.SCENARIO_DIR, None, tmp)
            self.assertTrue(summary["allPassed"], summary["runs"])
            self.assertEqual(len(summary["runs"]), len(scenarios))
            self.assertTrue((Path(tmp) / "index.json").exists())
            for entry in summary["runs"]:
                self.assertTrue(entry["passed"], entry["failedChecks"])
                self.assertEqual(entry["clientReport"]["failed"], 0)
                artifact = json.loads((Path(tmp) / entry["artifact"]).read_text(encoding="utf-8"))
                self.assertEqual(artifact["kind"], "metallic-local-mock-lab-run")
                self.assertEqual(artifact["labels"]["notActualGamePlayers"], True)
                self.assertTrue(artifact["verdict"]["noGhostSessions"])

    async def test_reconnect_scenario_exercises_rejoin_and_cleanup(self):
        metrics = MockRoomMetrics()
        server, players = await start_mock_server("127.0.0.1", 0, metrics)
        port = server.sockets[0].getsockname()[1]
        try:
            report = await run_scenario("127.0.0.1", port, clients=4, actions=6,
                                        seed=63, spawn_rate=40, mode="reconnect")
            self.assertEqual(report["failed"], 0)
            self.assertGreaterEqual(report["reconnects"], 1)
            self.assertGreater(metrics.snapshot()["successfulJoins"], 4)
            for _ in range(100):
                if not players:
                    break
                await asyncio.sleep(0.01)
            self.assertEqual(players, set())
        finally:
            server.close()
            await server.wait_closed()

    async def test_runner_refuses_remote_host(self):
        with self.assertRaises(ValueError):
            await run_lab.run_selected("203.0.113.99", 0, lab_policy.SCENARIO_DIR,
                                       ["smoke-local"], "unused-output")

    async def test_runner_rejects_unknown_scenario(self):
        with self.assertRaises(ValueError):
            await run_lab.run_selected("127.0.0.1", 0, lab_policy.SCENARIO_DIR,
                                       ["does-not-exist"], "unused-output")


if __name__ == "__main__":
    unittest.main()
