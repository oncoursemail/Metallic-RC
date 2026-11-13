#!/usr/bin/env python3
"""One bounded, explicitly elevated recovery experiment for the verified Steam build.

This program never self-elevates. --check-only performs hash/process/elevation
preflight without creating a shared section or launching a process. An actual
run requires the caller to have separately approved Windows elevation.

The named-section remedy is a build-specific public research hypothesis. This
wrapper tests it with the original client; it does not claim decoding success.
"""
from __future__ import annotations

import argparse
import ctypes as C
import datetime as DT
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import time

STEAM_ROOT = Path(r"C:\Program Files (x86)\Steam\steamapps\common\RecRoom")
GAMEASSEMBLY_SHA256 = "a6d90770cdfda4043eadb278a16198ed0907c3ba2ac09bc37d06cd4dd22a1d84"
RECROOM_SHA256 = "3bfddbd82167a7fd3042abaddd39b6ecaf2b7149407df606eb6af0e798a57a70"
CAPTURE_SHA256 = "37ace729f9ee7ce45b4189c6bc54e4f21a1b9f3063b6ee56a1570f4e4ddb5179"
SECTION_NAME = r"Global\KjMpQrStUvWxYzAb"
SECTION_BYTES = 66_060_456


def utc():
    return DT.datetime.now(DT.timezone.utc).isoformat()


def sha256(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for data in iter(lambda: f.read(1024 * 1024), b""):
            h.update(data)
    return h.hexdigest()


def json_write(path, value):
    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def verify_file(path, expected):
    result = {"path": str(path), "expectedSha256": expected, "exists": path.is_file()}
    if result["exists"]:
        result["bytes"] = path.stat().st_size
        result["sha256"] = sha256(path)
    result["matches"] = result.get("sha256") == expected
    return result


class Win32:
    def __init__(self):
        from ctypes import wintypes as W
        self.W = W
        self.k = C.WinDLL("kernel32", use_last_error=True)
        self.a = C.WinDLL("advapi32", use_last_error=True)
        self.u = C.WinDLL("user32", use_last_error=True)
        def api(dll, name, result, args):
            f = getattr(dll, name); f.restype = result; f.argtypes = args; return f
        self.close = api(self.k, "CloseHandle", W.BOOL, [C.c_void_p])
        self.current = api(self.k, "GetCurrentProcess", C.c_void_p, [])
        self.token = api(self.a, "OpenProcessToken", W.BOOL,
                         [C.c_void_p, W.DWORD, C.POINTER(C.c_void_p)])
        self.token_info = api(self.a, "GetTokenInformation", W.BOOL,
                              [C.c_void_p, C.c_int, C.c_void_p, W.DWORD, C.POINTER(W.DWORD)])
        self.mapping = api(self.k, "CreateFileMappingW", C.c_void_p,
                           [C.c_void_p, C.c_void_p, W.DWORD, W.DWORD, W.DWORD, W.LPCWSTR])
        self.enum_type = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
        self.enum_windows = api(self.u, "EnumWindows", W.BOOL, [self.enum_type, W.LPARAM])
        self.window_pid = api(self.u, "GetWindowThreadProcessId", W.DWORD,
                              [W.HWND, C.POINTER(W.DWORD)])
        self.post = api(self.u, "PostMessageW", W.BOOL, [W.HWND, W.UINT, W.WPARAM, W.LPARAM])

    def is_elevated(self):
        token = C.c_void_p()
        if not self.token(self.current(), 0x0008, C.byref(token)):
            raise C.WinError(C.get_last_error())
        try:
            flag, returned = self.W.DWORD(), self.W.DWORD()
            if not self.token_info(token, 20, C.byref(flag), C.sizeof(flag), C.byref(returned)):
                raise C.WinError(C.get_last_error())
            return bool(flag.value)
        finally:
            self.close(token)

    def create_new_section(self):
        # PAGE_READWRITE, paging-file backed. New mappings are initialized to zero
        # by Windows. No view is mapped, no existing section is written to.
        C.set_last_error(0)
        handle = self.mapping(C.c_void_p(-1), None, 0x04, 0, SECTION_BYTES, SECTION_NAME)
        error = C.get_last_error()
        if not handle:
            raise C.WinError(error)
        if error == 183:  # ERROR_ALREADY_EXISTS: do not use or modify it.
            self.close(handle)
            raise RuntimeError("Named section already exists; the experiment refuses to use it")
        return handle

    def request_window_close(self, pid):
        attempts = []
        @self.enum_type
        def visit(hwnd, _):
            owner = self.W.DWORD()
            self.window_pid(hwnd, C.byref(owner))
            if owner.value == pid:
                C.set_last_error(0)
                sent = self.post(hwnd, 0x0010, 0, 0)  # WM_CLOSE
                attempts.append({"hwnd": int(hwnd), "sent": bool(sent),
                                 "error": None if sent else C.get_last_error()})
            return True
        self.enum_windows(visit, 0)
        return attempts


def close_owned_process(proc, label, api, grace=3):
    """Only a retained Popen handle created by this wrapper may be terminated."""
    result = {"label": label, "pid": proc.pid, "startedByWrapper": True}
    if proc.poll() is None:
        result["windowCloseAttempts"] = api.request_window_close(proc.pid)
        try:
            proc.wait(timeout=grace)
        except subprocess.TimeoutExpired:
            # Popen.terminate uses our retained process handle, avoiding PID reuse.
            proc.terminate()
            result["terminatedAfterGrace"] = True
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                result["cleanupIncomplete"] = True
    result["exitCode"] = proc.poll()
    result["finishedUtc"] = utc()
    return result


def run(args):
    if os.name != "nt" or struct.calcsize("P") != 8:
        raise RuntimeError("Preflight and actual runs require 64-bit Python on Windows")
    folder = Path(__file__).resolve().parent
    helper = folder / "capture_steam_recroom.py"
    parent = Path(args.output_parent).resolve()
    steam = STEAM_ROOT.resolve()
    if parent == steam or steam in parent.parents:
        raise ValueError("Output parent must be outside the game installation")
    stamp = DT.datetime.now(DT.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    operation = parent / ("preflight-" if args.check_only else "recovery-") / stamp
    operation.mkdir(parents=True, exist_ok=False)
    receipt_path = operation / "operation-receipt.json"
    receipt = {"schemaVersion": 1, "startedUtc": utc(), "checkOnly": args.check_only,
               "status": "preflight", "wrapperPath": str(Path(__file__).resolve()),
               "wrapperSha256": sha256(__file__), "pythonPath": sys.executable,
               "steamRoot": str(steam), "outputDirectory": str(operation),
               "observationSeconds": args.seconds, "decoded": False,
               "selfElevationImplemented": False, "scriptWritesToGameFiles": False,
               "registryModified": False, "securitySettingsModified": False,
               "privilegesAdjusted": False, "persistenceCreated": False,
               "section": {"name": SECTION_NAME, "bytes": SECTION_BYTES,
                           "created": False, "wrapperHandleClosed": False,
                           "initialContents": "zero initialized by Windows if newly created"},
               "spawnedProcesses": [], "cleanup": [], "logFiles": [], "errors": [],
               "limitations": ["Build-specific shared-section hypothesis; not a verified decoder.",
                   "Capture heuristics can miss metadata or code; candidate outputs require offline validation.",
                   "The original game can create its ordinary logs or child processes during startup.",
                   "Cleanup only affects the two Popen handles created here, not unrelated processes or services.",
                   "Closing this section handle does not prove no game-held section references remain."]}
    mapping = None
    capture = None
    game = None
    handles = []
    api = None
    exit_code = 1
    def save():
        json_write(receipt_path, receipt)
    save()
    try:
        api = Win32()
        receipt["isElevated"] = api.is_elevated()
        checks = {
            "GameAssembly.dll": verify_file(steam / "GameAssembly.dll", GAMEASSEMBLY_SHA256),
            "RecRoom.exe": verify_file(steam / "RecRoom.exe", RECROOM_SHA256),
            "capture_steam_recroom.py": verify_file(helper, CAPTURE_SHA256),
        }
        receipt["preflightFiles"] = checks
        if not all(v["matches"] for v in checks.values()):
            raise RuntimeError("Preflight hash mismatch or missing file; see preflightFiles in receipt")
        # Only import the pinned, same-folder helper after its hash has matched.
        sys.dont_write_bytecode = True
        spec = importlib.util.spec_from_file_location("metallic_verified_capture", helper)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        process_api = module.WindowsReader()
        existing = process_api.processes()
        receipt["existingRecRoomPids"] = existing
        if existing:
            raise RuntimeError("Existing Rec Room process detected; no section or game process was created")
        receipt["preflightPassed"] = True
        if args.check_only:
            receipt["status"] = "preflight_passed" if receipt["isElevated"] else "preflight_passed_elevation_required"
            exit_code = 0
            return exit_code
        if not receipt["isElevated"]:
            raise PermissionError("Actual recovery requires a separately approved elevated Windows process; no self-elevation is attempted")
        mapping = api.create_new_section()
        receipt["section"].update({"created": True, "createdUtc": utc()})
        receipt["status"] = "section_created"
        save()
        # Refuse a race with someone independently opening another Rec Room client.
        if process_api.processes():
            raise RuntimeError("A Rec Room process appeared after preflight; refusing to launch")
        capture_dir = operation / "capture"
        for name in ("capture.stdout.log", "capture.stderr.log", "game.stdout.log", "game.stderr.log"):
            path = operation / name
            handles.append(path.open("wb"))
            receipt["logFiles"].append(name)
        pid_file = operation / "owned-game-pid.json"
        capture_command = [sys.executable, str(helper), "--output", str(capture_dir),
                           "--seconds", str(args.seconds), "--pid-file", str(pid_file)]
        capture = subprocess.Popen(capture_command, cwd=str(folder), stdin=subprocess.DEVNULL,
                                   stdout=handles[0], stderr=handles[1], shell=False)
        receipt["spawnedProcesses"].append({"kind": "capture", "pid": capture.pid,
                                            "command": capture_command, "startedUtc": utc()})
        receipt["status"] = "waiting_for_capture_readiness"
        save()
        readiness_deadline = time.monotonic() + 15
        capture_report = capture_dir / "capture-report.json"
        while time.monotonic() < readiness_deadline:
            if capture.poll() is not None:
                raise RuntimeError("Capture helper exited before readiness")
            if capture_report.exists():
                try:
                    ready = json.loads(capture_report.read_text(encoding="utf-8"))
                    if ready.get("status") == "observing" and ready.get("gameAssemblyDiskSha256") == GAMEASSEMBLY_SHA256:
                        receipt["captureReadyUtc"] = utc()
                        break
                except (OSError, ValueError):
                    pass
            time.sleep(0.05)
        else:
            raise TimeoutError("Capture helper did not signal readiness within 15 seconds")
        if process_api.processes():
            raise RuntimeError("A Rec Room process appeared before the controlled launch")
        game_command = [str(steam / "RecRoom.exe"), "+forcemode:screen"]
        game = subprocess.Popen(game_command, cwd=str(steam), stdin=subprocess.DEVNULL,
                                stdout=handles[2], stderr=handles[3], shell=False)
        json_write(pid_file, {"pid": game.pid, "executable": str(steam / "RecRoom.exe"),
                              "captureOutput": str(capture_dir), "gameAssemblySha256": GAMEASSEMBLY_SHA256,
                              "writtenUtc": utc()})
        receipt["ownedGamePidFile"] = str(pid_file)
        receipt["spawnedProcesses"].append({"kind": "game", "pid": game.pid,
                                            "command": game_command, "startedUtc": utc()})
        receipt["status"] = "observing"
        save()
        deadline = time.monotonic() + args.seconds
        while time.monotonic() < deadline and capture.poll() is None:
            time.sleep(min(0.1, max(0, deadline - time.monotonic())))
        receipt["observationFinishedUtc"] = utc()
        receipt["captureExitCodeAtObservationEnd"] = capture.poll()
        receipt["gameExitCodeAtObservationEnd"] = game.poll()
        # Normal helper exit may trail the deadline while it hashes/writes artifacts.
        if capture.poll() is None:
            try:
                capture.wait(timeout=5)
            except subprocess.TimeoutExpired:
                receipt["captureExceededGrace"] = True
        if capture_report.exists():
            try:
                result = json.loads(capture_report.read_text(encoding="utf-8"))
                receipt["captureSummary"] = {k: result.get(k) for k in
                    ("status", "completeMetadataCandidates", "completeMappedImages", "decoded")}
            except (OSError, ValueError) as e:
                receipt["errors"].append({"phase": "read_capture_summary", "type": type(e).__name__, "message": str(e)})
        receipt["status"] = "experiment_finished"
        exit_code = 0
        return exit_code
    except Exception as e:
        receipt["status"] = "failed"
        receipt["errors"].append({"utc": utc(), "type": type(e).__name__, "message": str(e)})
        return 1
    finally:
        # Keep the shared section alive until this wrapper's game has exited.
        try:
            for proc, label in ((game, "game"), (capture, "capture")):
                if proc is not None and api is not None:
                    try:
                        receipt["cleanup"].append(close_owned_process(proc, label, api))
                    except Exception as e:
                        receipt["errors"].append({"phase": "cleanup_" + label, "type": type(e).__name__, "message": str(e)})
            for index, handle in enumerate(handles):
                try:
                    handle.close()
                except Exception as e:
                    receipt["errors"].append({"phase": "close_log", "index": index,
                                              "type": type(e).__name__, "message": str(e)})
        finally:
            if mapping is not None and api is not None:
                try:
                    C.set_last_error(0)
                    closed = api.close(mapping)
                    receipt["section"].update({"wrapperHandleClosed": bool(closed), "closedUtc": utc(),
                                               "closeError": None if closed else C.get_last_error()})
                except Exception as e:
                    receipt["errors"].append({"phase": "close_section", "type": type(e).__name__, "message": str(e)})
        receipt["finishedUtc"] = utc()
        # Hash exact originals again after the experiment, including failed runs.
        receipt["postOperationFiles"] = {}
        for name, expected in (("GameAssembly.dll", GAMEASSEMBLY_SHA256), ("RecRoom.exe", RECROOM_SHA256)):
            try:
                receipt["postOperationFiles"][name] = verify_file(steam / name, expected)
            except OSError as e:
                receipt["postOperationFiles"][name] = {"error": str(e)}
        receipt["originalFileHashesMatchAfter"] = all(v.get("matches", False) for v in receipt["postOperationFiles"].values())
        receipt["originalFileHashScope"] = ["GameAssembly.dll", "RecRoom.exe"]
        receipt["logDigests"] = []
        for name in receipt["logFiles"]:
            path = operation / name
            if path.exists():
                receipt["logDigests"].append({"file": name, "bytes": path.stat().st_size, "sha256": sha256(path)})
        try:
            receipt["wrapperSha256After"] = sha256(__file__)
            receipt["captureHelperSha256After"] = sha256(helper) if helper.exists() else None
            save()
        finally:
            print(json.dumps({"status": receipt["status"], "receipt": str(receipt_path),
                              "sectionCreated": receipt["section"]["created"],
                              "sectionHandleClosed": receipt["section"]["wrapperHandleClosed"],
                              "decoded": False}), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-parent", required=True, help="Task-owned directory outside the Steam installation")
    parser.add_argument("--seconds", type=float, default=45)
    parser.add_argument("--check-only", action="store_true")
    args = parser.parse_args()
    if not 1 <= args.seconds <= 300:
        parser.error("--seconds must be between 1 and 300")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
