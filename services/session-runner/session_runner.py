#!/usr/bin/env python3
"""Supervise one checksum-verified, loopback-only Metallic-RC private playtest.

This Linux CLI verifies local build integrity, not publisher signatures. It has
no HTTP API, shell command surface, service installer, or public network mode.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import selectors
import signal
import stat
import subprocess
import sys
import threading
import time
import uuid

BUILD_SCHEMA = "metallic-rc-playtest-build-v1"
RECEIPT_SCHEMA = "metallic-rc-session-runner-v1"
BIND = "127.0.0.1"
TARGETS = {"server": "Metallic-RC-Server.x86_64", "linux": "Metallic-RC.x86_64"}
MAX_FILE_BYTES = 256 * 1024 * 1024
MAX_OUTPUT_BYTES = 1024 * 1024
MAX_LINE_BYTES = 4096
STATES = {"starting", "ready", "backoff", "stopped", "failed"}


class RunnerError(Exception):
    """A bounded reason code; never expose raw child output or environment."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class Config:
    manifest: Path
    data_dir: Path
    port: int = 27841
    max_attempts: int = 3
    ready_timeout: float = 10.0
    stop_timeout: float = 3.0
    backoff: float = 0.5
    backoff_cap: float = 8.0
    expected_manifest_sha256: str | None = None
    allow_root_for_tests: bool = False


@dataclass(frozen=True)
class Build:
    manifest: Path
    digest: str
    commit: str
    target: str
    source_dirty: bool
    binary: str
    pack: str
    files: dict


def _integer(value: object, minimum: int, maximum: int) -> bool:
    return type(value) is int and minimum <= value <= maximum


def validate_config(config: Config) -> None:
    if sys.platform != "linux":
        raise RunnerError("linux_required")
    if os.geteuid() == 0 and not config.allow_root_for_tests:
        raise RunnerError("root_refused")
    if not _integer(config.port, 1024, 65535):
        raise RunnerError("invalid_port")
    if not _integer(config.max_attempts, 1, 10):
        raise RunnerError("invalid_attempt_limit")
    for value, low, high in ((config.ready_timeout, 0.05, 60), (config.stop_timeout, 0.05, 10),
                             (config.backoff, 0.01, 10), (config.backoff_cap, 0.01, 30)):
        if type(value) not in (float, int) or not low <= value <= high or not math.isfinite(value):
            raise RunnerError("invalid_timeout")
    if config.backoff_cap < config.backoff:
        raise RunnerError("invalid_backoff")
    if config.expected_manifest_sha256 is not None and not _digest(config.expected_manifest_sha256):
        raise RunnerError("invalid_manifest_pin")


def _digest(value: object) -> bool:
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def _path(path: Path, *, missing_leaf: bool = False) -> Path:
    path = Path(path)
    if not path.is_absolute() or ".." in path.parts:
        raise RunnerError("absolute_safe_path_required")
    current = Path(path.anchor)
    for index, part in enumerate(path.parts[1:], 1):
        current /= part
        try:
            info = current.lstat()
        except FileNotFoundError:
            if missing_leaf and index == len(path.parts) - 1:
                return path
            raise RunnerError("missing_path") from None
        if stat.S_ISLNK(info.st_mode):
            raise RunnerError("symlink_refused")
        if index < len(path.parts) - 1 and not stat.S_ISDIR(info.st_mode):
            raise RunnerError("invalid_parent_directory")
    return path


def _check_file(info: os.stat_result, *, private: bool, limit: int, executable: bool = False) -> None:
    owners = {os.geteuid()} if private else {0, os.geteuid()}
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        raise RunnerError("regular_single_link_file_required")
    if info.st_uid not in owners:
        raise RunnerError("file_owner_mismatch")
    if info.st_mode & (0o077 if private else 0o022):
        raise RunnerError("unsafe_file_permissions")
    if not 0 < info.st_size <= limit:
        raise RunnerError("invalid_file_size")
    if executable and not info.st_mode & stat.S_IXUSR:
        raise RunnerError("executable_permission_required")


@contextmanager
def _read_file(path: Path, *, private: bool = False, limit: int = MAX_FILE_BYTES,
               executable: bool = False):
    _path(path)
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    except OSError:
        raise RunnerError("file_open_failed") from None
    try:
        before = os.fstat(fd)
        _check_file(before, private=private, limit=limit, executable=executable)
        with os.fdopen(fd, "rb", closefd=False) as stream:
            yield stream, before
        after = os.fstat(fd)
        if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
            raise RunnerError("file_changed_during_verification")
    finally:
        os.close(fd)


def _strict_json(raw: bytes) -> object:
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate key")
            result[key] = value
        return result

    def constant(_):
        raise ValueError("non-finite JSON")

    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=pairs, parse_constant=constant)
    except (UnicodeError, ValueError, RecursionError):
        raise RunnerError("invalid_json") from None


def _json_file(path: Path, *, private: bool = False) -> tuple[object, bytes]:
    with _read_file(path, private=private, limit=65536) as (stream, _):
        raw = stream.read(65537)
    if len(raw) > 65536:
        raise RunnerError("invalid_file_size")
    return _strict_json(raw), raw


def _hash_file(path: Path, expected: dict, *, executable: bool = False, private: bool = False) -> None:
    with _read_file(path, executable=executable, private=private) as (stream, info):
        if info.st_size != expected["bytes"]:
            raise RunnerError("build_size_mismatch")
        if _stream_digest(stream, expected["bytes"]) != expected["sha256"]:
            raise RunnerError("build_digest_mismatch")


def _stream_digest(stream, size: int, output=None) -> str:
    """Bound reads even if another process appends after the initial fstat."""
    remaining = size
    digest = hashlib.sha256()
    while remaining:
        chunk = stream.read(min(1024 * 1024, remaining))
        if not chunk:
            raise RunnerError("build_size_mismatch")
        if output is not None:
            output.write(chunk)
        digest.update(chunk)
        remaining -= len(chunk)
    if stream.read(1):
        raise RunnerError("build_size_mismatch")
    return digest.hexdigest()


def verify_build(manifest: Path, expected_digest: str | None = None) -> Build:
    manifest = _path(manifest)
    data, raw = _json_file(manifest)
    digest = hashlib.sha256(raw).hexdigest()
    if expected_digest is not None and digest != expected_digest:
        raise RunnerError("manifest_digest_mismatch")
    fields = {"schema", "project", "version", "commit", "source_dirty", "source_sha256", "godot",
              "status", "signed", "verification", "target", "template_sha256", "files"}
    if not isinstance(data, dict) or set(data) != fields:
        raise RunnerError("unsupported_manifest_fields")
    if data["schema"] != BUILD_SCHEMA or data["project"] != "Metallic-RC" or data["status"] != "private-development-build":
        raise RunnerError("unsupported_manifest")
    if data["signed"] is not False:
        raise RunnerError("unsupported_signature_claim")
    if not isinstance(data["target"], str) or data["target"] not in TARGETS:
        raise RunnerError("unsupported_build_target")
    if not isinstance(data["commit"], str) or not re.fullmatch(r"[0-9a-f]{40}", data["commit"]):
        raise RunnerError("invalid_build_commit")
    if type(data["source_dirty"]) is not bool or not _digest(data["source_sha256"]) or not _digest(data["template_sha256"]):
        raise RunnerError("invalid_build_provenance")
    for key in ("version", "godot", "verification"):
        if not isinstance(data[key], str) or not 1 <= len(data[key]) <= 512:
            raise RunnerError("invalid_build_metadata")
    binary = TARGETS[data["target"]]
    pack = str(Path(binary).with_suffix(".pck"))
    files = data["files"]
    if not isinstance(files, dict) or not {binary, pack} <= set(files):
        raise RunnerError("required_build_files_missing")
    if set(files) - {binary, pack, "README.txt", "THIRD_PARTY_NOTICES.txt", "GODOT_LICENSE.txt"}:
        raise RunnerError("unapproved_build_filename")
    for name, record in files.items():
        if not isinstance(record, dict) or set(record) != {"sha256", "bytes"}:
            raise RunnerError("invalid_file_manifest")
        if not _digest(record["sha256"]) or not _integer(record["bytes"], 1, MAX_FILE_BYTES):
            raise RunnerError("invalid_file_manifest")
        _hash_file(manifest.parent / name, record, executable=name == binary)
    return Build(manifest, digest, data["commit"], data["target"], data["source_dirty"], binary, pack, files)


def _private_directory(path: Path) -> None:
    _path(path, missing_leaf=True)
    try:
        path.mkdir(mode=0o700)
    except FileExistsError:
        pass
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
        raise RunnerError("private_owned_directory_required")


@contextmanager
def data_lock(data_dir: Path):
    _private_directory(data_dir)
    lock_path = data_dir / ".runner.lock"
    _path(lock_path, missing_leaf=True)
    fd = os.open(lock_path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077 or info.st_nlink != 1:
            raise RunnerError("unsafe_lock_file")
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RunnerError("data_directory_locked") from None
        yield
    finally:
        os.close(fd)


def validate_state(data_dir: Path) -> None:
    path = data_dir / "meadow.json"
    if not os.path.lexists(path):
        return
    data, _ = _json_file(path, private=True)
    keys = {"version", "round_id", "phase", "lit", "completed_by"}
    if not isinstance(data, dict) or set(data) != keys:
        raise RunnerError("unsupported_game_state")
    def json_integer(value, low, high):
        return type(value) in (int, float) and low <= value <= high and math.isfinite(value) and value == int(value)
    if not json_integer(data["version"], 1, 1) or not json_integer(data["round_id"], 1, 1000000000):
        raise RunnerError("unsupported_game_state")
    if data["phase"] not in ("collecting", "complete") or not isinstance(data["lit"], list):
        raise RunnerError("unsupported_game_state")
    lit = data["lit"]
    if len(lit) > 3 or any(type(item) is not str or item not in {"west", "east", "grove"} for item in lit) or len(set(lit)) != len(lit):
        raise RunnerError("unsupported_game_state")
    name = data["completed_by"]
    if not isinstance(name, str):
        raise RunnerError("unsupported_game_state")
    if data["phase"] == "collecting" and name != "":
        raise RunnerError("unsupported_game_state")
    if data["phase"] == "complete" and (len(lit) != 3 or name != name.strip() or re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9 _-]{1,19}", name) is None):
        raise RunnerError("unsupported_game_state")


def stage_build(build: Build, data_dir: Path) -> Path:
    """Run private verified copies, so package replacement cannot switch a live attempt."""
    cache = data_dir / "verified-builds"
    _private_directory(cache)
    stage = cache / build.digest
    _private_directory(stage)
    for name in (build.binary, build.pack):
        target = stage / name
        if os.path.lexists(target):
            _hash_file(target, build.files[name], executable=name == build.binary, private=True)
            continue
        temporary = stage / (".copy-" + uuid.uuid4().hex)
        try:
            with _read_file(build.manifest.parent / name, executable=name == build.binary) as (source, info):
                if info.st_size != build.files[name]["bytes"]:
                    raise RunnerError("build_size_mismatch")
                fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
                with os.fdopen(fd, "wb") as output:
                    digest = _stream_digest(source, build.files[name]["bytes"], output)
                    output.flush()
                    os.fsync(output.fileno())
                    os.fchmod(output.fileno(), 0o500 if name == build.binary else 0o400)
                if digest != build.files[name]["sha256"]:
                    raise RunnerError("build_digest_mismatch")
            os.replace(temporary, target)
        finally:
            if temporary.exists():
                temporary.unlink()
    return stage


def validate_readiness(line: bytes, port: int) -> None:
    data = _strict_json(line[len(b"METALLIC_READY "):])
    if not isinstance(data, dict) or set(data) != {"protocol", "port", "bind", "server"}:
        raise RunnerError("invalid_readiness")
    if type(data["protocol"]) is not int or data["protocol"] != 1 or type(data["port"]) is not int or data["port"] != port or data["bind"] != BIND or data["server"] is not True:
        raise RunnerError("invalid_readiness")


def validate_receipt(data_dir: Path) -> None:
    path = data_dir / "receipt.json"
    if os.path.lexists(path):
        previous, _ = _json_file(path, private=True)
        if not isinstance(previous, dict) or previous.get("schema") != RECEIPT_SCHEMA or not isinstance(previous.get("status"), str) or previous["status"] not in STATES:
            raise RunnerError("unsupported_runner_receipt")


class Supervisor:
    def __init__(self, config: Config, *, emit=None):
        self.config = config
        self.stop = threading.Event()
        self.emit = emit or (lambda data: print(json.dumps(data, sort_keys=True), flush=True))
        self.child: subprocess.Popen | None = None
        self.build: Build | None = None
        self.attempt = 0
        self.ready = False
        self.run_id = uuid.uuid4().hex

    def _receipt(self, status: str, reason: str, **extra) -> None:
        path = self.config.data_dir / "receipt.json"
        validate_receipt(self.config.data_dir)
        record = {
            "schema": RECEIPT_SCHEMA, "run_id": self.run_id, "status": status, "reason": reason,
            "updated_at": datetime.now(timezone.utc).isoformat(), "attempt": self.attempt,
            "max_attempts": self.config.max_attempts, "bind": BIND, "port": self.config.port,
            "pid": self.child.pid if self.child is not None and self.child.poll() is None else None,
            "ready": self.ready, "manifest_sha256": self.build.digest,
            "build_commit": self.build.commit, "source_dirty": self.build.source_dirty,
            "build_target": self.build.target, "verification": "sha256-integrity-only", "signed": False,
            "root_test_override": os.geteuid() == 0 and self.config.allow_root_for_tests,
            **extra,
        }
        temporary = path.with_name(".receipt-" + uuid.uuid4().hex)
        try:
            fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(record, stream, sort_keys=True)
                stream.write("\n")
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, path)
            directory_fd = os.open(self.config.data_dir, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
        finally:
            if temporary.exists():
                temporary.unlink()
        self.emit(record)

    def _terminate_child(self) -> None:
        child = self.child
        if child is None or child.poll() is not None:
            return
        # Never signal a stored PID, process name, process group, or another run.
        child.terminate()
        try:
            child.wait(timeout=self.config.stop_timeout)
        except subprocess.TimeoutExpired:
            child.kill()
            try:
                child.wait(timeout=self.config.stop_timeout)
            except subprocess.TimeoutExpired:
                raise RunnerError("termination_timeout") from None

    def _attempt(self, stage: Path) -> tuple[str, int | None]:
        runtime = self.config.data_dir / "runtime"
        _private_directory(runtime)
        environment = {"PATH": os.defpath, "LANG": "C.UTF-8", "TZ": "UTC"}
        for name in ("DATA", "CONFIG", "CACHE"):
            folder = runtime / name.lower()
            _private_directory(folder)
            environment["XDG_" + name + "_HOME"] = str(folder)
        command = [str(stage / self.build.binary), "--headless", "--", "--server", "--bind", BIND,
                   "--port", str(self.config.port), "--state-file", str(self.config.data_dir / "meadow.json")]
        self.ready = False
        try:
            self.child = subprocess.Popen(command, cwd=stage, env=environment, stdin=subprocess.DEVNULL,
                                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT, shell=False,
                                          close_fds=True, start_new_session=True, umask=0o077, bufsize=0)
        except OSError:
            raise RunnerError("spawn_failed") from None
        deadline = time.monotonic() + self.config.ready_timeout
        buffer = b""
        total = 0
        try:
            self._receipt("starting", "awaiting_readiness")
            with selectors.DefaultSelector() as selector:
                os.set_blocking(self.child.stdout.fileno(), False)
                selector.register(self.child.stdout, selectors.EVENT_READ)
                while True:
                    if self.stop.is_set():
                        return "stop_requested", None
                    for key, _ in selector.select(timeout=0.05):
                        chunk = os.read(key.fd, 8192)
                        if not chunk:
                            selector.unregister(key.fileobj)
                            continue
                        total += len(chunk)
                        if total > MAX_OUTPUT_BYTES:
                            raise RunnerError("child_output_budget_exceeded")
                        buffer += chunk
                        while b"\n" in buffer:
                            line, buffer = buffer.split(b"\n", 1)
                            if len(line) > MAX_LINE_BYTES:
                                raise RunnerError("child_output_line_too_long")
                            if line.startswith(b"METALLIC_READY "):
                                if self.ready:
                                    raise RunnerError("duplicate_readiness")
                                validate_readiness(line, self.config.port)
                                if self.child.poll() is not None:
                                    return "exit_before_ready", self.child.returncode
                                self.ready = True
                                self._receipt("ready", "readiness_validated")
                        if len(buffer) > MAX_LINE_BYTES:
                            raise RunnerError("child_output_line_too_long")
                    code = self.child.poll()
                    if code is not None:
                        return ("child_exited" if self.ready else "exit_before_ready"), code
                    if not self.ready and time.monotonic() >= deadline:
                        return "readiness_timeout", None
        finally:
            self._terminate_child()
            self.child.stdout.close()

    def run(self) -> int:
        validate_config(self.config)
        self.build = verify_build(self.config.manifest, self.config.expected_manifest_sha256)
        with data_lock(self.config.data_dir):
            validate_receipt(self.config.data_dir)
            try:
                for attempt in range(1, self.config.max_attempts + 1):
                    self.attempt = attempt
                    self.ready = False
                    if self.stop.is_set():
                        self._receipt("stopped", "stop_requested")
                        return 0
                    # Pin the manifest for the whole lifecycle and recheck both originals
                    # and staged bytes before every restart. Never silently adopt updates.
                    self.build = verify_build(self.config.manifest, self.build.digest)
                    validate_state(self.config.data_dir)
                    stage = stage_build(self.build, self.config.data_dir)
                    reason, exit_code = self._attempt(stage)
                    self.ready = False
                    if self.stop.is_set():
                        self._receipt("stopped", "stop_requested")
                        return 0
                    if attempt == self.config.max_attempts:
                        self._receipt("failed", "attempt_limit", last_reason=reason, exit_code=exit_code)
                        return 1
                    delay = min(self.config.backoff_cap, self.config.backoff * 2 ** (attempt - 1))
                    self._receipt("backoff", reason, delay_seconds=delay, exit_code=exit_code)
                    if self.stop.wait(delay):
                        self._receipt("stopped", "stop_requested")
                        return 0
            except RunnerError as error:
                self.ready = False
                self._receipt("failed", error.code)
                return 1
        return 1


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run", help="Run one private loopback session until stopped or exhausted")
    run.add_argument("--manifest", type=Path, required=True, help="Absolute path to reviewed BUILD.json")
    run.add_argument("--data-dir", type=Path, required=True, help="Absolute owner-private session directory")
    run.add_argument("--port", type=int, default=27841, help="One fixed unprivileged loopback port")
    run.add_argument("--max-attempts", type=int, default=3)
    run.add_argument("--ready-timeout", type=float, default=10.0)
    run.add_argument("--stop-timeout", type=float, default=3.0)
    run.add_argument("--backoff", type=float, default=0.5)
    run.add_argument("--backoff-cap", type=float, default=8.0)
    run.add_argument("--expected-manifest-sha256", help="Optional separately verified manifest digest pin")
    run.add_argument("--allow-root-for-tests", action="store_true", help="Explicit test-only root override; never a production setting")
    args = vars(parser.parse_args(argv))
    args.pop("command")
    supervisor = Supervisor(Config(**args))
    previous = {}
    for signum in (signal.SIGINT, signal.SIGTERM):
        previous[signum] = signal.signal(signum, lambda *_: supervisor.stop.set())
    try:
        return supervisor.run()
    except RunnerError as error:
        print(json.dumps({"schema": RECEIPT_SCHEMA, "status": "rejected", "reason": error.code}), file=sys.stderr)
        return 2
    except OSError:
        print(json.dumps({"schema": RECEIPT_SCHEMA, "status": "rejected", "reason": "filesystem_or_process_error"}), file=sys.stderr)
        return 2
    finally:
        supervisor._terminate_child()
        for signum, handler in previous.items():
            signal.signal(signum, handler)


if __name__ == "__main__":
    raise SystemExit(main())
