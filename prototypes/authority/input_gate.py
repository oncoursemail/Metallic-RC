"""REFERENCE ONLY: input admission checks for a future authoritative server.

This is NOT an anti-cheat product or an integrated game server. The caller must
authenticate the session, enforce game-specific physics/ownership/action rules,
use server-owned ticks, and apply additional DDoS/network controls. No kernel
driver or client OS dependence, to keep Linux and Windows policy consistent.
"""
from __future__ import annotations

from dataclasses import dataclass
import math


@dataclass(frozen=True)
class InputCommand:
    sequence: int
    strafe: float
    forward: float
    action: str = "move"


@dataclass(frozen=True)
class Decision:
    accepted: bool
    reason: str


@dataclass
class _SessionState:
    last_sequence: int = -1
    last_tick: int = -1
    accepted_this_tick: int = 0


class InputGate:
    """Bounds input spam, replays and malformed commands before simulation.

    Only an authenticated *server-assigned session key* should be registered
    with connect(). The client must never select someone else's session key.
    """

    DEFAULT_ACTIONS = frozenset({"move", "idle", "jump", "emote", "grab", "interact"})

    def __init__(self, max_commands_per_tick: int = 4,
                 max_sequence_advance: int = 32,
                 allowed_actions: frozenset[str] | None = None) -> None:
        if isinstance(max_commands_per_tick, bool) or not isinstance(max_commands_per_tick, int) or max_commands_per_tick <= 0:
            raise ValueError("max_commands_per_tick must be positive integer")
        if isinstance(max_sequence_advance, bool) or not isinstance(max_sequence_advance, int) or max_sequence_advance <= 0:
            raise ValueError("max_sequence_advance must be positive integer")
        self.max_commands_per_tick = max_commands_per_tick
        self.max_sequence_advance = max_sequence_advance
        self.allowed_actions = self.DEFAULT_ACTIONS if allowed_actions is None else frozenset(allowed_actions)
        self._sessions: dict[str, _SessionState] = {}

    def connect(self, authenticated_session_key: str) -> None:
        """Call only AFTER transport/identity authenticates a *new* session."""
        if not isinstance(authenticated_session_key, str) or not authenticated_session_key:
            raise ValueError("Authenticated session key must be nonempty string")
        if authenticated_session_key in self._sessions:
            raise ValueError("Duplicate session registration rejected")
        self._sessions[authenticated_session_key] = _SessionState()

    def disconnect(self, authenticated_session_key: str) -> None:
        self._sessions.pop(authenticated_session_key, None)

    def inspect(self, authenticated_session_key: str, command: InputCommand,
                *, server_tick: int) -> Decision:
        """Reject invalid inputs WITHOUT changing the last accepted sequence.

        Server tick must originate from the trusted fixed-step simulator, not
        from packet data. Accepted command means *safe to evaluate further*,
        never automatically authorizes any action, object, reward or inventory.
        """
        state = self._sessions.get(authenticated_session_key)
        if state is None:
            return Decision(False, "unknown_session")
        if isinstance(server_tick, bool) or not isinstance(server_tick, int) or server_tick < 0:
            return Decision(False, "invalid_server_tick")
        if state.last_tick > server_tick:
            return Decision(False, "server_tick_regression")
        if not isinstance(command, InputCommand):
            return Decision(False, "invalid_command_shape")
        if (isinstance(command.sequence, bool) or not isinstance(command.sequence, int)
                or command.sequence < 0):
            return Decision(False, "invalid_sequence")
        if command.sequence <= state.last_sequence:
            return Decision(False, "duplicate_or_replayed_sequence")
        if command.sequence - state.last_sequence > self.max_sequence_advance:
            return Decision(False, "sequence_jump_too_large")
        if not isinstance(command.action, str) or command.action not in self.allowed_actions:
            return Decision(False, "unrecognized_or_forbidden_action")
        for value in (command.strafe, command.forward):
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
                return Decision(False, "invalid_move_axis")
            if abs(value) > 1:
                return Decision(False, "move_axis_exceeds_bounds")
        if command.strafe ** 2 + command.forward ** 2 > 1.000001:
            return Decision(False, "unnormalized_move_vector")
        if state.last_tick == server_tick and state.accepted_this_tick >= self.max_commands_per_tick:
            return Decision(False, "command_rate_exceeded")

        if state.last_tick < server_tick:
            state.accepted_this_tick = 0
        state.accepted_this_tick += 1
        state.last_sequence = command.sequence
        state.last_tick = server_tick
        return Decision(True, "accepted_for_server_simulation")
