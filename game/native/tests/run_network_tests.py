#!/usr/bin/env python3
"""Run real Godot ENet clients against the native authority, with recorded evidence.

This is deliberately separate from the repository's synthetic tick/load model. Each
bot is an actual Godot process sending production RPCs through ENet. The optional
UDP relay delays/drops real datagrams; it does not change simulation state.
"""
from __future__ import annotations

import argparse
import heapq
import json
import os
from pathlib import Path
import platform
import random
import selectors
import socket
import subprocess
import threading
import time
from dataclasses import dataclass
from typing import Any, Callable


@dataclass
class Peer:
    process: subprocess.Popen[bytes]
    log_path: Path
    result_path: Path
    stream: Any

    def joined(self) -> bool:
        return "METALLIC_BOT_JOINED " in self.log_path.read_text(errors="replace")

    def ready(self) -> bool:
        return "METALLIC_READY " in self.log_path.read_text(errors="replace")

    def result(self) -> dict[str, Any]:
        if not self.result_path.exists():
            raise AssertionError(f"Missing {self.result_path.name}; {self.log_path.read_text(errors='replace')[-3000:]}")
        return json.loads(self.result_path.read_text())


class DatagramRelay:
    """Bounded loopback-only UDP impairment relay; source mapping is per client."""

    def __init__(self, server_port: int, delay_ms: float, loss_percent: float, seed: int = 20261020):
        self.server = ("127.0.0.1", server_port)
        self.delay = delay_ms / 1000.0
        self.loss = loss_percent / 100.0
        self.random = random.Random(seed)
        self.listener = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.listener.bind(("127.0.0.1", 0))
        self.listener.setblocking(False)
        self.port = self.listener.getsockname()[1]
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.listener, selectors.EVENT_READ, None)
        self.clients: dict[tuple[str, int], socket.socket] = {}
        self.pending: list[tuple[float, int, socket.socket, bytes, tuple[str, int]]] = []
        self.counter = 0
        self.datagrams = 0
        self.dropped = 0
        self.delivered = 0
        self.errors: list[str] = []
        self.stopping = threading.Event()
        self.thread = threading.Thread(target=self._run, name="metallic-loopback-impairment", daemon=True)

    def start(self) -> None:
        self.thread.start()

    def _queue(self, outgoing: socket.socket, data: bytes, destination: tuple[str, int]) -> None:
        self.datagrams += 1
        if self.random.random() < self.loss:
            self.dropped += 1
            return
        if len(self.pending) >= 20000:
            self.errors.append("relay_queue_limit")
            self.dropped += 1
            return
        self.counter += 1
        jitter = self.random.uniform(-self.delay / 4, self.delay / 4)
        heapq.heappush(self.pending, (time.monotonic() + self.delay + jitter, self.counter, outgoing, data, destination))

    def _run(self) -> None:
        while not self.stopping.is_set():
            try:
                for key, _events in self.selector.select(timeout=0.004):
                    data, sender = key.fileobj.recvfrom(65536)
                    if key.data is None:
                        if sender not in self.clients:
                            if len(self.clients) >= 64:
                                self.errors.append("relay_client_limit")
                                continue
                            upstream = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                            upstream.bind(("127.0.0.1", 0))
                            upstream.setblocking(False)
                            self.clients[sender] = upstream
                            self.selector.register(upstream, selectors.EVENT_READ, sender)
                        self._queue(self.clients[sender], data, self.server)
                    elif sender == self.server:
                        self._queue(self.listener, data, key.data)
                now = time.monotonic()
                while self.pending and self.pending[0][0] <= now:
                    _at, _seq, outgoing, payload, destination = heapq.heappop(self.pending)
                    outgoing.sendto(payload, destination)
                    self.delivered += 1
            except (BlockingIOError, ConnectionResetError):
                continue
            except OSError as error:
                if not self.stopping.is_set():
                    self.errors.append(type(error).__name__)

    def close(self) -> None:
        self.stopping.set()
        self.thread.join(timeout=2)
        self.selector.close()
        self.listener.close()
        for upstream in self.clients.values():
            upstream.close()

    def report(self) -> dict[str, Any]:
        return {
            "enabled": True,
            "configured_one_way_delay_ms": self.delay * 1000,
            "configured_jitter_ms": self.delay * 250,
            "configured_packet_loss_percent": self.loss * 100,
            "datagrams_seen": self.datagrams,
            "datagrams_delivered": self.delivered,
            "datagrams_dropped": self.dropped,
            "errors": self.errors,
            "seed": 20261020,
        }


class Harness:
    def __init__(self, godot: Path, output: Path):
        self.godot = godot.resolve()
        self.project = Path(__file__).resolve().parents[1]
        self.output = output.resolve()
        self.output.mkdir(parents=True, exist_ok=True)
        self.processes: list[Peer] = []
        self.environment = os.environ.copy()
        for key, subdir in (("XDG_DATA_HOME", "xdg-data"), ("XDG_CONFIG_HOME", "xdg-config"), ("XDG_CACHE_HOME", "xdg-cache")):
            folder = self.output / subdir
            folder.mkdir(exist_ok=True)
            self.environment[key] = str(folder)

    @staticmethod
    def free_port() -> int:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as udp:
            udp.bind(("127.0.0.1", 0))
            return int(udp.getsockname()[1])

    def launch(self, label: str, **parameters: object) -> Peer:
        log_path = self.output / f"{label}.log"
        result_path = self.output / f"{label}.json"
        result_path.unlink(missing_ok=True)
        command = [str(self.godot), "--headless", "--audio-driver", "Dummy", "--path", str(self.project), "--max-fps", "60", "res://tests/network_peer.tscn", "--", "--result-file", str(result_path)]
        for key, value in parameters.items():
            command += ["--" + key.replace("_", "-"), str(value)]
        stream = log_path.open("wb")
        process = subprocess.Popen(command, cwd=self.project, env=self.environment, stdout=stream, stderr=subprocess.STDOUT)
        peer = Peer(process, log_path, result_path, stream)
        self.processes.append(peer)
        return peer

    @staticmethod
    def wait(predicate: Callable[[], bool], timeout: float, description: str, peers: list[Peer] | None = None) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            for peer in peers or []:
                if peer.process.poll() is not None:
                    raise AssertionError(f"Peer exited during {description}: {peer.log_path.read_text(errors='replace')[-3000:]}")
            time.sleep(0.1)
        raise AssertionError(f"Timed out waiting for {description}")

    def finish(self, peer: Peer, timeout: float = 10) -> dict[str, Any]:
        try:
            code = peer.process.wait(timeout=timeout)
        except subprocess.TimeoutExpired as error:
            raise AssertionError(f"Peer did not finish: {peer.log_path.name}") from error
        peer.stream.flush()
        result = peer.result()
        if code != 0 or not result.get("ok"):
            raise AssertionError(f"Scenario failed: {json.dumps(result, sort_keys=True)}; {peer.log_path.read_text(errors='replace')[-3000:]}")
        log = peer.log_path.read_text(errors="replace")
        if any(line.startswith(("ERROR:", "SCRIPT ERROR:")) for line in log.splitlines()):
            raise AssertionError(f"Engine or script error in {peer.log_path.name}: {log[-3000:]}")
        return result

    def stop(self, peer: Peer) -> None:
        if peer.process.poll() is None:
            peer.process.terminate()
            try:
                peer.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                peer.process.kill()
                peer.process.wait(timeout=3)

    def close(self) -> None:
        for peer in self.processes:
            self.stop(peer)
            peer.stream.close()

    def activity_and_recovery(self, delay_ms: float, loss_percent: float) -> dict[str, Any]:
        port = self.free_port()
        state = self.output / "meadow-state.json"
        state.unlink(missing_ok=True)
        telemetry = self.output / "activity-server-telemetry.json"
        server = self.launch("activity-server", role="server", port=port, duration=110, state_file=state, telemetry_file=telemetry)
        self.wait(server.ready, 10, "dedicated server readiness", [server])
        protocol = self.launch("protocol-probe", role="bot", mode="bad_protocol", port=port, duration=10)
        protocol_result = self.finish(protocol, 12)
        input_probe = self.launch("input-guard-probe", role="bot", mode="input_probe", name="InputGuardProbe", port=port, duration=12)
        input_probe_result = self.finish(input_probe, 14)
        expected_input_cases = {"valid_baseline", "replayed_sequence", "older_sequence", "oversized_axis", "wrong_axis_type", "spoofed_member", "sequence_leap", "non_finite_axis"}
        assert set(input_probe_result["input_guard_cases_acknowledged"]) == expected_input_cases, "Every actual input guard must be acknowledged by the authority"
        relay = DatagramRelay(port, delay_ms, loss_percent)
        relay.start()
        try:
            actor = self.launch("activity-actor", role="bot", mode="actor", name="MeadowActor", port=relay.port, duration=65, reconnect="true")
            self.wait(actor.joined, 12, "first real client admission", [actor, server])
            observer = self.launch("activity-observer", role="bot", mode="observer", name="MeadowObserver", port=port, duration=65)
            self.wait(observer.joined, 10, "second real client admission", [observer, server])
            actor_result = self.finish(actor, 65)
            observer_result = self.finish(observer, 10)
        finally:
            relay.close()
        assert actor_result["round"] == observer_result["round"], "Both actual clients must agree on the complete shared round"
        assert actor_result["resumed_same_player"] and actor_result["resume_progress_retained"], "Live-server resumption must preserve player and progress"
        assert actor_result["distance_rejection_observed"] and actor_result["server_rejections_observed"], "Actual server must reject forbidden actions and input"
        assert actor_result["client_reset_api_attempted"] and actor_result["round"]["id"] == 1, "Calling the client reset API must leave the authority round unchanged"
        assert relay.report()["datagrams_dropped"] > 0 and not relay.errors, "Impairment must affect actual ENet datagrams without relay failure"
        progress = json.loads(state.read_text())
        assert progress["phase"] == "complete" and set(progress["lit"]) == {"west", "east", "grove"}
        assert "token" not in state.read_text().lower(), "Resume secrets must not enter progress files"
        if os.name != "nt":
            assert state.stat().st_mode & 0o777 == 0o600, "Progress save must have private Unix permissions"
        activity_telemetry = json.loads(telemetry.read_text())
        recovery = self.launch("crash-recovery-client", role="bot", mode="recovery", name="RecoveryClient", port=port, duration=50, expected_joins=3)
        self.wait(recovery.joined, 10, "pre-crash client admission", [recovery, server])
        restart_stop = self.output / "restart.stop"
        restart_stop.unlink(missing_ok=True)
        current_server = server
        restarted_servers: list[Peer] = []
        restart_metrics: list[dict[str, Any]] = []
        for restart_index in range(2):
            # Forced termination bypasses graceful shutdown on both Windows and Unix.
            current_server.process.kill()
            current_server.process.wait(timeout=5)
            restart_telemetry = self.output / f"restarted-server-{restart_index + 1}-telemetry.json"
            restarted = self.launch(f"restarted-server-{restart_index + 1}", role="server", port=port, duration=40, state_file=state, stop_file=restart_stop, telemetry_file=restart_telemetry)
            restarted_servers.append(restarted)
            self.wait(restarted.ready, 10, "crash-restored server readiness", [restarted])
            required_joins = restart_index + 2
            self.wait(lambda: recovery.log_path.read_text(errors="replace").count("METALLIC_BOT_JOINED ") >= required_joins, 20, f"automatic rejoin after consecutive restart {restart_index + 1}", [recovery, restarted])
            restart_metrics.append(json.loads(restart_telemetry.read_text())["metrics"])
            current_server = restarted
        recovery_result = self.finish(recovery, 8)
        assert recovery_result["crash_recovery_round_retained"] and recovery_result["join_count"] == 3 and recovery_result["crash_recovery_count"] == 2
        assert recovery_result["round"]["id"] == progress["round_id"]
        assert len(recovery_result["recovery_rounds"]) == 3 and all(round_state["id"] == progress["round_id"] and round_state["phase"] == "complete" and len(round_state["lit"]) == 3 for round_state in recovery_result["recovery_rounds"])
        restart_stop.write_text("stop\n")
        restart_result = self.finish(current_server, 5)
        corrupt = self.output / "corrupt-state.json"
        corrupt.write_text('{"version": 1, "lit": ["admin"]}\n')
        original_corrupt = corrupt.read_bytes()
        corrupt_server = self.launch("corrupt-state-probe", role="server", port=self.free_port(), duration=2, state_file=corrupt)
        corrupt_code = corrupt_server.process.wait(timeout=8)
        assert corrupt_code != 0 and not corrupt_server.ready(), "Corrupt state must prevent server startup"
        assert corrupt.read_bytes() == original_corrupt, "Corrupt state must be preserved for inspection"
        assert not any(any(line.startswith(("ERROR:", "SCRIPT ERROR:")) for line in peer.log_path.read_text(errors="replace").splitlines()) for peer in [server, *restarted_servers, corrupt_server]), "Server logs must be free of engine and script errors"
        return {
            "ok": True,
            "client_count_during_activity": 2,
            "protocol_probe": protocol_result,
            "input_guard_probe": input_probe_result,
            "actor": actor_result,
            "observer": observer_result,
            "impairment": relay.report(),
            "server_metrics": activity_telemetry["metrics"],
            "crash_recovery": recovery_result,
            "restarted_server_metrics": restart_result["metrics"],
            "consecutive_crash_restarts": 2,
            "restart_metrics": restart_metrics,
            "persisted_progress": progress,
            "corrupt_state_startup_refused": True,
            "corrupt_state_preserved": True,
        }

    def capacity(self, clients: int, duration: float) -> dict[str, Any]:
        port = self.free_port()
        server_stop = self.output / "load-server.stop"
        client_stop = self.output / "load-clients.stop"
        server_stop.unlink(missing_ok=True)
        client_stop.unlink(missing_ok=True)
        telemetry = self.output / "load-server-telemetry.json"
        server = self.launch("load-server", role="server", port=port, duration=70, stop_file=server_stop, telemetry_file=telemetry)
        self.wait(server.ready, 10, "load server readiness", [server])
        bots = [self.launch(f"load-bot-{index:02d}", role="bot", mode="load", name=f"LoadBot{index:02d}", index=index, port=port, duration=55, expected_players=clients, stop_file=client_stop) for index in range(clients)]
        self.wait(lambda: all(bot.joined() for bot in bots), 25, f"{clients} real ENet clients", [server] + bots)
        def full_snapshot() -> bool:
            try:
                return json.loads(telemetry.read_text())["snapshot"]["server"]["connected_count"] == clients
            except (OSError, ValueError, KeyError):
                return False
        self.wait(full_snapshot, 5, "authority snapshot at target concurrency", [server] + bots)
        print(f"METALLIC_TEST_PROGRESS {clients} real ENet bots admitted; measuring {duration:.0f}s", flush=True)
        overflow_result: dict[str, Any] | None = None
        if clients == 32:
            overflow = self.launch("capacity-overflow", role="bot", mode="expect_full", name="OverflowClient", port=port, duration=12)
            overflow_result = self.finish(overflow, 14)
            assert overflow_result["capacity_denied"]
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            if any(bot.process.poll() is not None for bot in bots):
                raise AssertionError("A load bot disconnected or exited before the measurement ended")
            time.sleep(0.1)
        peak = json.loads(telemetry.read_text())
        assert peak["snapshot"]["server"]["connected_count"] == clients
        assert peak["metrics"]["accepted_inputs"] >= clients * 30, "Each bot must send real accepted inputs"
        client_stop.write_text("stop\n")
        results = [self.finish(bot, 8) for bot in bots]
        assert all(result["moved_metres"] > 0.5 and result["speed_bound_passed"] for result in results)
        assert all(result["peak_players"] >= clients for result in results), "Every real client must observe target concurrency"
        server_stop.write_text("stop\n")
        self.finish(server, 5)
        return {"ok": True, "real_client_processes": clients, "measurement_seconds_after_all_admitted": duration, "server_metrics": peak["metrics"], "all_clients_saw_target_concurrency": all(result["peak_players"] >= clients for result in results), "all_clients_observed_bounded_movement": True, "overflow": overflow_result, "clients": results}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--godot", type=Path, required=True, help="Official Godot 4.5.2 executable")
    parser.add_argument("--output", type=Path, required=True, help="Evidence directory, outside tracked source recommended")
    parser.add_argument("--clients", type=int, default=32, choices=range(2, 33))
    parser.add_argument("--load-seconds", type=float, default=8.0)
    parser.add_argument("--delay-ms", type=float, default=35.0)
    parser.add_argument("--loss-percent", type=float, default=2.0)
    parser.add_argument("--skip-load", action="store_true")
    parser.add_argument("--load-only", action="store_true")
    args = parser.parse_args()
    if args.skip_load and args.load_only:
        parser.error("--skip-load and --load-only cannot be used together")
    if not args.godot.is_file() or not 0 <= args.delay_ms <= 100 or not 0 < args.loss_percent <= 10 or not 3 <= args.load_seconds <= 30:
        parser.error("Use an existing Godot binary, delay0..100ms, loss>0..10%, and load duration3..30s")
    harness = Harness(args.godot, args.output)
    report: dict[str, Any] = {
        "schema_version": 1,
        "transport": "real Godot ENet RPCs, separate OS client processes",
        "synthetic_tick_model": False,
        "platform": platform.platform(),
        "processor": platform.processor(),
        "logical_cpus": os.cpu_count(),
        "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "limits": ["Loopback traffic on the single reported platform; not an Internet, other-platform, GPU, VR, or public-service certification", "Headless bots do not measure rendered client performance", "Tick costs are authority callback wall-clock microseconds; they are not end-to-end latency or total system CPU", "Process-crash recovery tested; filesystem power-loss durability is not established"],
    }
    started = time.monotonic()
    try:
        if not args.load_only:
            print("METALLIC_TEST_PROGRESS Running two actual ENet clients, impaired datagrams, resumption, and process-crash restore", flush=True)
            report["activity_and_recovery"] = harness.activity_and_recovery(args.delay_ms, args.loss_percent)
        if not args.skip_load:
            report["capacity"] = harness.capacity(args.clients, args.load_seconds)
        report["ok"] = True
    except (AssertionError, OSError, subprocess.SubprocessError) as error:
        report["ok"] = False
        report["failure"] = str(error)
    finally:
        harness.close()
        report["elapsed_seconds"] = round(time.monotonic() - started, 3)
        destination = harness.output / "network-evidence.json"
        destination.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print("METALLIC_NETWORK_SUITE " + json.dumps({"ok": report["ok"], "elapsed_seconds": report["elapsed_seconds"], "evidence": str(destination), "failure": report.get("failure")}), flush=True)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
