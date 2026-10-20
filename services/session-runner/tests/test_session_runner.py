"""Real finite child-process tests; no Godot, network listener, or installed service."""
from __future__ import annotations

from dataclasses import replace
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

MODULE = Path(__file__).resolve().parents[1] / "session_runner.py"
spec = importlib.util.spec_from_file_location("metallic_session_runner", MODULE)
runner = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = runner
spec.loader.exec_module(runner)

FIXTURE = r'''
import json, signal, sys, time
from pathlib import Path
args = sys.argv[1:]
assert args[:4] == ['--headless', '--', '--server', '--bind']
assert args[4] == '127.0.0.1' and args[5] == '--port' and args[7] == '--state-file' and len(args) == 9
port = int(args[6])
state = Path(args[8])
data = state.parent
config = json.loads(Path(__file__).with_suffix('.pck').read_text())
mode = config['mode']
counter = data / 'fixture-attempts'
attempt = int(counter.read_text()) + 1 if counter.exists() else 1
counter.write_text(str(attempt))
def stopped(*_):
    (data / 'fixture-stopped').write_text('stopped')
    raise SystemExit(0)
signal.signal(signal.SIGTERM, signal.SIG_IGN if mode == 'ignore-stop' else stopped)
ready = {'protocol': 1, 'port': port, 'bind': '127.0.0.1', 'server': True}
if mode == 'wrong-port':
    ready['port'] += 1
if mode == 'wrong-bind':
    ready['bind'] = '0.0.0.0'
if mode == 'invalid-state':
    state.write_text('{"version":999}')
    raise SystemExit(17)
if mode == 'flood':
    print('PRIVATE_FIXTURE_VALUE=' + 'x' * 6000, flush=True)
if mode == 'exit-before':
    raise SystemExit(17)
if mode == 'crash-first' and attempt == 1:
    state.write_text(json.dumps({'version': 1, 'round_id': 7, 'phase': 'collecting', 'lit': ['west'], 'completed_by': ''}))
if mode == 'crash-first' and attempt == 2:
    assert json.loads(state.read_text())['lit'] == ['west']
    (data / 'fixture-recovered').write_text('round 7 restored')
if mode != 'no-ready':
    print('METALLIC_READY ' + json.dumps(ready), flush=True)
if mode == 'crash-always' or (mode == 'crash-first' and attempt == 1):
    time.sleep(0.04)
    raise SystemExit(17)
if mode == 'duplicate-ready':
    print('METALLIC_READY ' + json.dumps(ready), flush=True)
while True:
    time.sleep(0.01)
'''


class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="metallic-runner-test-")
        self.root = Path(self.temporary.name)
        self.package = self.root / "package"
        self.package.mkdir(mode=0o700)
        self.data = self.root / "session"
        self.binary = self.package / "Metallic-RC-Server.x86_64"
        self.pack = self.binary.with_suffix(".pck")
        self.binary.write_text("#!" + sys.executable + "\n" + FIXTURE)
        self.binary.chmod(0o700)
        self.pack.write_text(json.dumps({"mode": "ready"}))
        self.manifest = self.package / "BUILD.json"
        self.write_manifest()
        self.config = runner.Config(self.manifest, self.data, max_attempts=3,
                                    ready_timeout=0.4, stop_timeout=0.1, backoff=0.03, backoff_cap=0.06,
                                    allow_root_for_tests=True)

    def tearDown(self):
        self.temporary.cleanup()

    def write_manifest(self):
        self.record = {
            "schema": runner.BUILD_SCHEMA, "project": "Metallic-RC", "version": "0.1.0-playtest.1",
            "commit": "a" * 40, "source_dirty": False, "source_sha256": "b" * 64,
            "godot": "fixture-only", "status": "private-development-build", "signed": False,
            "verification": "Finite test fixture, not a Godot package.", "target": "server",
            "template_sha256": "c" * 64,
            "files": {p.name: {"sha256": hashlib.sha256(p.read_bytes()).hexdigest(), "bytes": p.stat().st_size}
                      for p in (self.binary, self.pack)},
        }
        self.manifest.write_text(json.dumps(self.record) + "\n")

    def mode(self, mode):
        self.pack.write_text(json.dumps({"mode": mode}))
        self.write_manifest()

    def state(self, **changes):
        self.data.mkdir(mode=0o700, exist_ok=True)
        data = {"version": 1, "round_id": 7, "phase": "collecting", "lit": ["west"], "completed_by": ""}
        data.update(changes)
        path = self.data / "meadow.json"
        path.write_text(json.dumps(data))
        path.chmod(0o600)
        return path

    def execute(self, *, stop_on_ready=True, after=None, config=None):
        events = []
        supervisor = None
        def event(record):
            events.append(record)
            if after:
                after(supervisor, record)
            elif stop_on_ready and record["status"] == "ready":
                supervisor.stop.set()
        supervisor = runner.Supervisor(config or self.config, emit=event)
        result = supervisor.run()
        self.assertTrue(supervisor.child is None or supervisor.child.poll() is not None)
        return result, events, supervisor

    def test_verified_binary_and_pack_run_with_fixed_arguments_and_no_shell(self):
        with mock.patch.object(runner.subprocess, "Popen", wraps=subprocess.Popen) as spawn:
            result, events, _ = self.execute()
        self.assertEqual(result, 0)
        self.assertEqual([e["status"] for e in events], ["starting", "ready", "stopped"])
        self.assertEqual(events[-1]["pid"], None)
        self.assertFalse(events[-1]["signed"])
        args, kwargs = spawn.call_args
        self.assertFalse(kwargs["shell"])
        self.assertEqual(args[0][1:8], ["--headless", "--", "--server", "--bind", "127.0.0.1", "--port", "27841"])
        self.assertEqual(Path(args[0][0]).parent.name, events[-1]["manifest_sha256"])
        self.assertNotIn("GITHUB_TOKEN", kwargs["env"])
        self.assertEqual((self.data / "receipt.json").stat().st_mode & 0o777, 0o600)
        self.assertEqual((self.data / "fixture-stopped").read_text(), "stopped")

    def test_crash_restarts_once_and_preserves_the_shared_round(self):
        self.mode("crash-first")
        def after(supervisor, event):
            if event["status"] == "ready" and event["attempt"] == 2:
                supervisor.stop.set()
        result, events, _ = self.execute(after=after)
        self.assertEqual(result, 0)
        self.assertEqual((self.data / "fixture-attempts").read_text(), "2")
        self.assertEqual((self.data / "fixture-recovered").read_text(), "round 7 restored")
        self.assertEqual(json.loads((self.data / "meadow.json").read_text())["lit"], ["west"])
        self.assertEqual([e["delay_seconds"] for e in events if e["status"] == "backoff"], [0.03])

    def test_restart_limit_and_backoff_are_bounded_and_persisted(self):
        self.mode("crash-always")
        start = time.monotonic()
        result, events, _ = self.execute(stop_on_ready=False)
        self.assertEqual(result, 1)
        self.assertEqual((self.data / "fixture-attempts").read_text(), "3")
        self.assertEqual([e["delay_seconds"] for e in events if e["status"] == "backoff"], [0.03, 0.06])
        self.assertGreaterEqual(time.monotonic() - start, 0.09)
        receipt = json.loads((self.data / "receipt.json").read_text())
        self.assertEqual((receipt["status"], receipt["reason"], receipt["attempt"]), ("failed", "attempt_limit", 3))

    def test_build_change_blocks_restart_before_a_second_child_starts(self):
        self.mode("crash-always")
        def after(_, event):
            if event["status"] == "backoff":
                self.pack.write_text("tampered")
        result, events, _ = self.execute(after=after)
        self.assertEqual(result, 1)
        self.assertEqual((self.data / "fixture-attempts").read_text(), "1")
        self.assertIn(events[-1]["reason"], {"build_size_mismatch", "build_digest_mismatch"})

    def test_readiness_must_match_fixed_port_bind_and_schema(self):
        for mode in ("wrong-port", "wrong-bind", "duplicate-ready"):
            with self.subTest(mode=mode):
                self.mode(mode)
                result, events, _ = self.execute(stop_on_ready=False)
                self.assertEqual(result, 1)
                self.assertIn(events[-1]["reason"], {"invalid_readiness", "duplicate_readiness"})
                self.assertEqual(events[-1]["attempt"], 1)

    def test_ready_timeout_terminates_child_and_reports_failure(self):
        self.mode("no-ready")
        result, events, _ = self.execute(config=replace(self.config, max_attempts=1, ready_timeout=0.1))
        self.assertEqual(result, 1)
        self.assertEqual(events[-1]["last_reason"], "readiness_timeout")

    def test_exit_before_ready_never_reports_ready(self):
        self.mode("exit-before")
        result, events, _ = self.execute(config=replace(self.config, max_attempts=1))
        self.assertEqual(result, 1)
        self.assertNotIn("ready", [e["status"] for e in events])
        self.assertEqual(events[-1]["last_reason"], "exit_before_ready")

    def test_oversized_child_output_is_bounded_and_never_copied_to_receipt(self):
        self.mode("flood")
        result, events, _ = self.execute(stop_on_ready=False)
        self.assertEqual(result, 1)
        self.assertEqual(events[-1]["reason"], "child_output_line_too_long")
        self.assertNotIn("PRIVATE_FIXTURE_VALUE", json.dumps(events))
        self.assertLess((self.data / "receipt.json").stat().st_size, 2000)

    def test_shutdown_kills_only_owned_child_when_it_ignores_term(self):
        self.mode("ignore-stop")
        sibling = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"],
                                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            result, _, supervisor = self.execute()
            self.assertEqual(result, 0)
            self.assertEqual(supervisor.child.returncode, -signal.SIGKILL)
            self.assertIsNone(sibling.poll())
        finally:
            sibling.terminate()
            sibling.wait(timeout=2)

    def test_corrupt_game_state_after_crash_is_preserved_and_stops_retries(self):
        self.mode("invalid-state")
        result, events, _ = self.execute(stop_on_ready=False)
        self.assertEqual(result, 1)
        self.assertEqual(events[-1]["reason"], "unsupported_game_state")
        self.assertEqual((self.data / "fixture-attempts").read_text(), "1")
        self.assertEqual((self.data / "meadow.json").read_text(), '{"version":999}')

    def test_manifest_checks_missing_tampered_and_wrong_size_pack(self):
        for path in (self.binary, self.pack):
            with self.subTest(file=path.name):
                original = path.read_bytes()
                mode = path.stat().st_mode & 0o777
                path.unlink()
                with self.assertRaises(runner.RunnerError):
                    runner.verify_build(self.manifest)
                path.write_bytes(b"x" * len(original))
                path.chmod(mode)
                with self.assertRaisesRegex(runner.RunnerError, "build_digest_mismatch"):
                    runner.verify_build(self.manifest)
                path.write_bytes(original + b"extra")
                with self.assertRaisesRegex(runner.RunnerError, "build_size_mismatch"):
                    runner.verify_build(self.manifest)
                path.write_bytes(original)

    def test_actual_export_manifest_includes_verified_license_and_readme_files(self):
        for name in ("README.txt", "THIRD_PARTY_NOTICES.txt", "GODOT_LICENSE.txt"):
            path = self.package / name
            path.write_text("Test fixture notice for " + name + "\n")
            self.record["files"][name] = {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
        self.manifest.write_text(json.dumps(self.record) + "\n")
        build = runner.verify_build(self.manifest)
        self.assertEqual(set(build.files), {self.binary.name, self.pack.name, "README.txt", "THIRD_PARTY_NOTICES.txt", "GODOT_LICENSE.txt"})
        result, _, _ = self.execute()
        self.assertEqual(result, 0)
        (self.package / "GODOT_LICENSE.txt").write_text("tampered")
        with self.assertRaises(runner.RunnerError):
            runner.verify_build(self.manifest)

    def test_hashing_rejects_growth_after_stat_without_unbounded_reading(self):
        class GrowingFile:
            read_bytes = 0
            def read(self, count):
                self.read_bytes += count
                return b"x" * count
        stream = GrowingFile()
        with self.assertRaisesRegex(runner.RunnerError, "build_size_mismatch"):
            runner._stream_digest(stream, 1024)
        self.assertEqual(stream.read_bytes, 1025)
        with self.assertRaisesRegex(runner.RunnerError, "build_size_mismatch"):
            runner._stream_digest(io.BytesIO(b"short"), 1024)

    def test_manifest_pin_detects_rewritten_manifest_even_with_valid_files(self):
        digest = hashlib.sha256(self.manifest.read_bytes()).hexdigest()
        runner.verify_build(self.manifest, digest)
        self.manifest.write_text(json.dumps(self.record, indent=2))
        with self.assertRaisesRegex(runner.RunnerError, "manifest_digest_mismatch"):
            runner.verify_build(self.manifest, digest)

    def test_manifest_refuses_unknown_commands_paths_targets_and_signing_claims(self):
        changes = [lambda d: d.update(command="arbitrary shell"),
                   lambda d: d["files"].update({"../outside": {"sha256": "a" * 64, "bytes": 1}}),
                   lambda d: d.update(target="windows"), lambda d: d.update(target=[]),
                   lambda d: d.update(signed=True),
                   lambda d: d["files"].pop(self.pack.name),
                   lambda d: d["files"][self.pack.name].update(bytes=True)]
        for change in changes:
            self.write_manifest()
            change(self.record)
            self.manifest.write_text(json.dumps(self.record))
            with self.assertRaises(runner.RunnerError):
                runner.verify_build(self.manifest)

    def test_json_duplicates_nonfinite_and_boolean_readiness_are_rejected(self):
        for raw in (b'{"x":1,"x":2}', b'{"x":NaN}', b'{"x":Infinity}'):
            with self.assertRaises(runner.RunnerError):
                runner._strict_json(raw)
        for record in ({"protocol": True, "port": 27841, "bind": runner.BIND, "server": True},
                       {"protocol": 1, "port": True, "bind": runner.BIND, "server": True},
                       {"protocol": 1, "port": 27841, "bind": runner.BIND, "server": 1}):
            with self.assertRaises(runner.RunnerError):
                runner.validate_readiness(b"METALLIC_READY " + json.dumps(record).encode(), 27841)

    def test_symlink_build_and_symlink_data_parent_are_rejected(self):
        hidden = self.root / "hidden.pck"
        self.pack.rename(hidden)
        self.pack.symlink_to(hidden)
        with self.assertRaisesRegex(runner.RunnerError, "symlink_refused"):
            runner.verify_build(self.manifest)
        alias = self.root / "alias"
        alias.symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(runner.RunnerError, "symlink_refused"):
            with runner.data_lock(alias / "session"):
                pass

    def test_state_ownership_permissions_hardlinks_and_symlinks_are_rejected(self):
        path = self.state()
        path.chmod(0o644)
        with self.assertRaisesRegex(runner.RunnerError, "unsafe_file_permissions"):
            runner.validate_state(self.data)
        path.chmod(0o600)
        os.link(path, self.data / "alias-state")
        with self.assertRaisesRegex(runner.RunnerError, "regular_single_link_file_required"):
            runner.validate_state(self.data)
        (self.data / "alias-state").unlink()
        with mock.patch.object(runner.os, "geteuid", return_value=123456):
            with self.assertRaisesRegex(runner.RunnerError, "file_owner_mismatch"):
                runner.validate_state(self.data)
        original = self.data / "original-state"
        path.rename(original)
        path.symlink_to(original)
        with self.assertRaisesRegex(runner.RunnerError, "symlink_refused"):
            runner.validate_state(self.data)

    def test_state_schema_and_round_invariants_match_native_contract(self):
        self.state()
        runner.validate_state(self.data)
        self.state(phase="complete", lit=["west", "east", "grove"], completed_by="Alex")
        runner.validate_state(self.data)
        invalid = [{"version": 2}, {"version": True}, {"round_id": 1.5}, {"round_id": 10 ** 500},
                   {"phase": "other"}, {"lit": ["west", "west"]}, {"lit": ["unknown"]},
                   {"lit": [{}]}, {"completed_by": "Player"},
                   {"phase": "complete", "lit": ["west"], "completed_by": "Alex"},
                   {"phase": "complete", "lit": ["west", "east", "grove"], "completed_by": "<bad>"}]
        for changes in invalid:
            self.state(**changes)
            before = (self.data / "meadow.json").read_bytes()
            with self.assertRaises(runner.RunnerError):
                runner.validate_state(self.data)
            self.assertEqual((self.data / "meadow.json").read_bytes(), before)

    def test_data_directory_lock_is_exclusive_and_recoverable(self):
        with runner.data_lock(self.data):
            with self.assertRaisesRegex(runner.RunnerError, "data_directory_locked"):
                with runner.data_lock(self.data):
                    pass
        with runner.data_lock(self.data):
            pass
        with mock.patch.object(runner.os, "geteuid", return_value=123456):
            with self.assertRaisesRegex(runner.RunnerError, "private_owned_directory_required"):
                with runner.data_lock(self.data):
                    pass

    def test_unsupported_old_receipt_is_rejected_before_launch(self):
        self.data.mkdir(mode=0o700)
        receipt = self.data / "receipt.json"
        receipt.write_text(json.dumps({"schema": "unknown", "status": []}))
        receipt.chmod(0o600)
        with mock.patch.object(runner.subprocess, "Popen") as spawn:
            with self.assertRaisesRegex(runner.RunnerError, "unsupported_runner_receipt"):
                runner.Supervisor(self.config, emit=lambda _: None).run()
            spawn.assert_not_called()

    def test_root_and_invalid_runtime_limits_fail_before_execution(self):
        with mock.patch.object(runner.os, "geteuid", return_value=0):
            with self.assertRaisesRegex(runner.RunnerError, "root_refused"):
                runner.validate_config(replace(self.config, allow_root_for_tests=False))
        for changes in ({"port": 0}, {"port": 80}, {"port": 65536}, {"max_attempts": 11},
                        {"ready_timeout": float("inf")}, {"backoff": 0}, {"backoff_cap": 0.01}):
            with self.assertRaises(runner.RunnerError):
                runner.validate_config(replace(self.config, **changes))

    def test_cli_sigterm_stops_owned_child_and_leaves_a_stopped_receipt(self):
        command = [sys.executable, str(MODULE), "run", "--manifest", str(self.manifest), "--data-dir", str(self.data),
                   "--ready-timeout", "1", "--stop-timeout", "0.2", "--allow-root-for-tests"]
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            ready = False
            deadline = time.monotonic() + 5
            with selectors.DefaultSelector() as selector:
                selector.register(process.stdout, selectors.EVENT_READ)
                while not ready and time.monotonic() < deadline and process.poll() is None:
                    for _, _ in selector.select(timeout=0.1):
                        line = process.stdout.readline()
                        ready = bool(line) and json.loads(line)["status"] == "ready"
            self.assertTrue(ready)
            process.send_signal(signal.SIGTERM)
            stdout, stderr = process.communicate(timeout=3)
            self.assertEqual(process.returncode, 0, stderr)
            self.assertEqual(json.loads((self.data / "receipt.json").read_text())["status"], "stopped")
            self.assertTrue((self.data / "fixture-stopped").is_file())
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=3)
            process.stdout.close()
            process.stderr.close()


if __name__ == "__main__":
    unittest.main()
