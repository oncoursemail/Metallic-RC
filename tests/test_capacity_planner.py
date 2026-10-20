"""Regression checks for the assumption-only Metallic capacity model."""
import copy
import importlib.util
import math
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "tools" / "capacity_planner.py"
SPEC = importlib.util.spec_from_file_location("metallic_capacity_planner", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
capacity_planner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(capacity_planner)


def example():
    return {
        "regions": [
            {"name": "americas", "peak_ccu": 1000},
            {"name": "europe", "peak_ccu": 50}
        ],
        "assumptions": {
            "players_per_session": 32,
            "cpu_cores_per_session": 1,
            "ram_gib_per_session": 2,
            "cpu_cores_per_host": 12,
            "ram_gib_per_host": 32,
            "reserved_cpu_cores_per_host": 2,
            "reserved_ram_gib_per_host": 4,
            "capacity_headroom_fraction": 0.2,
            "spare_hosts_per_active_region": 1,
            "server_to_client_game_kbps": 80,
            "server_to_client_voice_kbps": 20,
            "egress_overhead_fraction": 0.1,
            "average_to_peak_ccu_fraction": 0.5,
            "hours_in_month": 720
        }
    }


class TestCapacityEstimator(unittest.TestCase):
    def test_rounds_sessions_and_hosts_per_region(self):
        result = capacity_planner.estimate(example())
        self.assertEqual(result["assumed_peak_ccu"], 1050)
        self.assertEqual(result["sessions_per_host_model"], 8)
        self.assertEqual(result["total_sessions"], 34)
        self.assertEqual(result["total_hosts_including_regional_spares"], 7)
        self.assertEqual(result["per_region"][0]["required_sessions"], 32)
        self.assertEqual(result["per_region"][1]["required_sessions"], 2)
        self.assertEqual(result["per_region"][0]["active_hosts"], 4)
        self.assertEqual(result["per_region"][1]["active_hosts"], 1)
        self.assertEqual(result["result_type"], "HYPOTHETICAL_CAPACITY_MODEL_NOT_MEASURED")

    def test_bandwidth_and_egress_math(self):
        x = example()
        result = capacity_planner.estimate(x)
        # (80+20) kbps * 1.1 overhead * 1050 clients = 115.5 Mbps peak.
        self.assertAlmostEqual(result["model_peak_egress_mbps"], 115.5)
        self.assertAlmostEqual(result["model_average_egress_mbps"], 57.75)
        expected_gib = 57.75 * 1_000_000 / 8 * 720 * 3600 / 1024 ** 3
        self.assertAlmostEqual(result["model_monthly_egress_gib"], expected_gib, places=2)
        self.assertIsNone(result["model_monthly_egress_cost_usd"])

    def test_egress_cost_is_explicit_not_implied(self):
        x = example()
        x["assumptions"]["egress_usd_per_gib"] = 0.10
        result = capacity_planner.estimate(x)
        self.assertAlmostEqual(
            result["model_monthly_egress_cost_usd"],
            round(result["model_monthly_egress_gib"] * 0.10, 2),
            delta=0.01,
        )

    def test_zero_region_requires_no_active_host_or_spare(self):
        x = example()
        x["regions"] = [{"name": "inactive", "peak_ccu": 0}]
        result = capacity_planner.estimate(x)
        self.assertEqual(result["total_sessions"], 0)
        self.assertEqual(result["total_hosts_including_regional_spares"], 0)

    def test_large_memory_sessions_limit_host_capacity(self):
        x = example()
        x["assumptions"]["ram_gib_per_session"] = 12
        self.assertEqual(capacity_planner.estimate(x)["sessions_per_host_model"], 1)

    def test_rejects_infinite_negative_and_boolean_values(self):
        for field, value in [
            ("players_per_session", -1),
            ("cpu_cores_per_host", math.inf),
            ("ram_gib_per_host", -4),
            ("server_to_client_game_kbps", True),
            ("average_to_peak_ccu_fraction", 1.1),
            ("capacity_headroom_fraction", 1.0),
            ("egress_overhead_fraction", -0.1),
        ]:
            with self.subTest(field=field):
                x = copy.deepcopy(example())
                x["assumptions"][field] = value
                with self.assertRaises(ValueError):
                    capacity_planner.estimate(x)

    def test_rejects_duplicate_regions(self):
        x = example()
        x["regions"][1]["name"] = "americas"
        with self.assertRaises(ValueError):
            capacity_planner.estimate(x)

    def test_rejects_oversubscribed_host(self):
        x = example()
        x["assumptions"]["cpu_cores_per_session"] = 100
        with self.assertRaises(ValueError):
            capacity_planner.estimate(x)

    def test_rejects_invalid_types(self):
        x = example()
        x["regions"] = "us-central"
        with self.assertRaises(ValueError):
            capacity_planner.estimate(x)
        x = example()
        x["regions"][0]["peak_ccu"] = 3.5
        with self.assertRaises(ValueError):
            capacity_planner.estimate(x)


if __name__ == "__main__":
    unittest.main()
