"""Tests for a reference-only, OS-neutral authoritative input gate."""
import importlib.util
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "prototypes" / "authority" / "input_gate.py"
SPEC = importlib.util.spec_from_file_location("metallic_authority_gate", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
import sys
module = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = module
SPEC.loader.exec_module(module)
InputCommand = module.InputCommand
InputGate = module.InputGate


class TestInputGate(unittest.TestCase):
    def setUp(self):
        self.gate = InputGate(max_commands_per_tick=2)
        self.gate.connect("server-verified-session")

    def inspect(self, sequence, tick=10, strafe=0, forward=1, action="move"):
        return self.gate.inspect(
            "server-verified-session",
            InputCommand(sequence, strafe, forward, action),
            server_tick=tick,
        )

    def test_accepts_normalized_intent_but_not_gameplay_awards(self):
        self.assertTrue(self.inspect(0, strafe=0.6, forward=0.8).accepted)
        self.assertFalse(self.inspect(1, action="grant_currency").accepted)
        self.assertEqual(self.inspect(1, action="grant_currency").reason,
                         "unrecognized_or_forbidden_action")

    def test_duplicate_and_old_sequences_rejected(self):
        self.assertTrue(self.inspect(0).accepted)
        self.assertEqual(self.inspect(0).reason, "duplicate_or_replayed_sequence")
        self.assertEqual(self.inspect(-1).reason, "invalid_sequence")

    def test_invalid_packet_does_not_advance_sequence(self):
        self.assertEqual(self.inspect(0, strafe=float("nan")).reason, "invalid_move_axis")
        self.assertTrue(self.inspect(0, strafe=0).accepted)

    def test_rejects_invalid_axes_and_diagonal_speed(self):
        self.assertEqual(self.inspect(0, strafe=1, forward=1).reason, "unnormalized_move_vector")
        self.assertEqual(self.inspect(0, strafe=float("inf")).reason, "invalid_move_axis")
        self.assertEqual(self.inspect(0, forward=True).reason, "invalid_move_axis")
        self.assertEqual(self.inspect(0, forward=2).reason, "move_axis_exceeds_bounds")

    def test_rate_limit_per_server_owned_tick(self):
        self.assertTrue(self.inspect(0, tick=10).accepted)
        self.assertTrue(self.inspect(1, tick=10).accepted)
        self.assertEqual(self.inspect(2, tick=10).reason, "command_rate_exceeded")
        self.assertTrue(self.inspect(2, tick=11).accepted)

    def test_stale_server_tick_is_rejected(self):
        self.assertTrue(self.inspect(0, tick=10).accepted)
        self.assertEqual(self.inspect(1, tick=9).reason, "server_tick_regression")

    def test_jump_in_sequence_is_bounded(self):
        self.assertEqual(self.inspect(100).reason, "sequence_jump_too_large")
        self.assertTrue(self.inspect(0).accepted)

    def test_session_identity_not_supplied_by_packet(self):
        bad = self.gate.inspect("not-authenticated", InputCommand(0, 0, 1), server_tick=10)
        self.assertEqual(bad.reason, "unknown_session")

    def test_duplicate_registration_does_not_reset_replay_counter(self):
        self.assertTrue(self.inspect(0).accepted)
        with self.assertRaises(ValueError):
            self.gate.connect("server-verified-session")
        self.assertFalse(self.inspect(0).accepted)

    def test_new_authenticated_session_has_independent_state(self):
        self.gate.connect("different-authenticated-session")
        self.assertTrue(self.inspect(0).accepted)
        other = self.gate.inspect("different-authenticated-session", InputCommand(0, 0, 1), server_tick=10)
        self.assertTrue(other.accepted)

    def test_session_revocation(self):
        self.gate.disconnect("server-verified-session")
        self.assertEqual(self.inspect(0).reason, "unknown_session")

    def test_rejects_boolean_sequence(self):
        self.assertEqual(self.inspect(True).reason, "invalid_sequence")


if __name__ == "__main__":
    unittest.main()
