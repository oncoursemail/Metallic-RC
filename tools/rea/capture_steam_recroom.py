#!/usr/bin/env python3
"""Read-only, narrowly scoped runtime evidence capture for the inspected Steam build.

This helper is NOT a decoder. It records candidate metadata and one GameAssembly
mapped image with coverage/provenance so an offline decoder can validate them.
It never launches/suspends a process, writes target memory, enables privileges,
changes game files, or reads unrelated processes. Run --self-test on any OS.
"""
from __future__ import annotations

import argparse
import collections
import ctypes as C
import datetime as DT
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import sys
import time

STEAM_ROOT = r"C:\Program Files (x86)\Steam\steamapps\common\RecRoom"
NAMES = {"recroom.exe", "recroom_release.exe"}
METADATA_BYTES = 52_403_096
GAMEASSEMBLY_SHA256 = "a6d90770cdfda4043eadb278a16198ed0907c3ba2ac09bc37d06cd4dd22a1d84"
PAGE = 4096
CHUNK = 1024 * 1024


def utc():
    return DT.datetime.now(DT.timezone.utc).isoformat()


def digest_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(CHUNK), b""):
            h.update(b)
    return h.hexdigest()


def entropy(data):
    if not data:
        return 0.0
    n = len(data)
    return -sum((v / n) * math.log2(v / n) for v in collections.Counter(data).values())


def pe_layout(data):
    """Parse only a bounded AMD64 PE32+ header; never trust unbounded offsets."""
    if len(data) < 64 or data[:2] != b"MZ":
        raise ValueError("Missing DOS header")
    pe = struct.unpack_from("<I", data, 0x3c)[0]
    if not 64 <= pe <= 0x10000 or pe + 24 > len(data) or data[pe:pe+4] != b"PE\0\0":
        raise ValueError("Invalid PE header offset/signature")
    machine, count = struct.unpack_from("<HH", data, pe + 4)
    optsize = struct.unpack_from("<H", data, pe + 20)[0]
    opt = pe + 24
    if machine != 0x8664 or not 1 <= count <= 96 or optsize < 112:
        raise ValueError("Not a supported AMD64 PE32+ image")
    if opt + optsize + 40 * count > len(data):
        raise ValueError("Truncated section headers")
    if struct.unpack_from("<H", data, opt)[0] != 0x20b:
        raise ValueError("Not PE32+")
    imagebase = struct.unpack_from("<Q", data, opt + 24)[0]
    imagesize, headersize = struct.unpack_from("<II", data, opt + 56)
    if not PAGE <= imagesize <= 1024 * 1024 * 1024 or not 1 <= headersize <= imagesize:
        raise ValueError("Unreasonable image/header size")
    sections = []
    for i in range(count):
        pos = opt + optsize + 40 * i
        name = bytes(data[pos:pos+8]).split(b"\0", 1)[0].decode("ascii", "replace")
        vs, rva, rawsize, rawptr = struct.unpack_from("<IIII", data, pos + 8)
        flags = struct.unpack_from("<I", data, pos + 36)[0]
        if rva + vs > imagesize:
            raise ValueError("Section outside mapped image")
        sections.append({"name": name, "rva": rva, "virtualSize": vs,
                         "rawSize": rawsize, "rawOffset": rawptr, "characteristics": flags})
    return {"machine": "AMD64", "imageBase": imagebase, "imageSize": imagesize,
            "headerSize": headersize, "sections": sections}


def header_candidate(data, expected=METADATA_BYTES):
    """Screen a permuted 256-byte header; this deliberately does not identify tables."""
    if len(data) < 256:
        return {"plausible": False, "reason": "short_header"}
    h = bytes(data[:256])
    values = struct.unpack("<64I", h)
    bounded = sum(0 <= v <= expected for v in values)
    distinct = len(set(values))
    plausible = len(set(h)) > 4 and 256 in values and bounded >= 54 and distinct >= 20
    return {"plausible": plausible, "boundedHeaderValues": bounded,
            "distinctHeaderValues": distinct, "contains256": 256 in values,
            "uniform52": h == b"R" * 256,
            "standardMagicPresent": values[0] == 0xFAB11BAF}


def append_range(ranges, start, end, error=None):
    if start >= end:
        return
    item = {"offset": start, "length": end - start}
    if error is not None:
        item["error"] = error
    if ranges and ranges[-1]["offset"] + ranges[-1]["length"] == start and ranges[-1].get("error") == error:
        ranges[-1]["length"] += end - start
    else:
        ranges.append(item)


def read_span(reader, base, size, deadline):
    """Try 1 MiB reads, retry incomplete suffixes at page boundaries, record every hole."""
    out = bytearray(size)
    good, holes = [], []
    pos = 0
    while pos < size:
        if time.monotonic() >= deadline:
            append_range(holes, pos, size, "deadline")
            break
        requested = min(CHUNK, size - pos)
        data, err = reader(base + pos, requested)
        got = min(len(data), requested)
        if got:
            out[pos:pos+got] = data[:got]
            append_range(good, pos, pos + got)
        end = pos + requested
        pos += got
        if got == requested:
            continue
        while pos < end:
            if time.monotonic() >= deadline:
                append_range(holes, pos, size, "deadline")
                pos = size
                break
            want = min(PAGE - ((base + pos) % PAGE), end - pos)
            part, page_error = reader(base + pos, want)
            taken = min(len(part), want)
            if taken:
                out[pos:pos+taken] = part[:taken]
                append_range(good, pos, pos + taken)
            if taken < want:
                append_range(holes, pos + taken, pos + want, page_error or err or "short_read")
            pos += want
    return out, {"requestedBytes": size, "readBytes": sum(r["length"] for r in good),
                 "complete": not holes, "readRanges": good, "unreadableRanges": holes,
                 "holeRepresentation": "zero-filled; coverage must be consulted"}


class WindowsReader:
    def __init__(self):
        from ctypes import wintypes as W
        self.W = W
        self.k = C.WinDLL("kernel32", use_last_error=True)
        class PE(C.Structure):
            _fields_ = [("dwSize", W.DWORD), ("cntUsage", W.DWORD), ("pid", W.DWORD),
                        ("heap", C.c_size_t), ("module", W.DWORD), ("threads", W.DWORD),
                        ("parent", W.DWORD), ("priority", W.LONG), ("flags", W.DWORD),
                        ("name", W.WCHAR * 260)]
        class ME(C.Structure):
            _fields_ = [("dwSize", W.DWORD), ("moduleID", W.DWORD), ("pid", W.DWORD),
                        ("globalUsage", W.DWORD), ("processUsage", W.DWORD),
                        ("base", C.c_void_p), ("size", W.DWORD), ("module", C.c_void_p),
                        ("name", W.WCHAR * 256), ("path", W.WCHAR * 260)]
        class MBI(C.Structure):
            _fields_ = [("base", C.c_void_p), ("allocationBase", C.c_void_p),
                        ("allocationProtect", W.DWORD), ("partition", W.WORD),
                        ("padding", W.WORD), ("size", C.c_size_t), ("state", W.DWORD),
                        ("protect", W.DWORD), ("type", W.DWORD), ("padding2", W.DWORD)]
        self.PE, self.ME, self.MBI = PE, ME, MBI
        def api(name, result, args):
            f = getattr(self.k, name); f.restype = result; f.argtypes = args; return f
        self.snapshot = api("CreateToolhelp32Snapshot", C.c_void_p, [W.DWORD, W.DWORD])
        self.close = api("CloseHandle", W.BOOL, [C.c_void_p])
        self.pfirst = api("Process32FirstW", W.BOOL, [C.c_void_p, C.POINTER(PE)])
        self.pnext = api("Process32NextW", W.BOOL, [C.c_void_p, C.POINTER(PE)])
        self.mfirst = api("Module32FirstW", W.BOOL, [C.c_void_p, C.POINTER(ME)])
        self.mnext = api("Module32NextW", W.BOOL, [C.c_void_p, C.POINTER(ME)])
        self.open = api("OpenProcess", C.c_void_p, [W.DWORD, W.BOOL, W.DWORD])
        self.image = api("QueryFullProcessImageNameW", W.BOOL,
                         [C.c_void_p, W.DWORD, W.LPWSTR, C.POINTER(W.DWORD)])
        self.query = api("VirtualQueryEx", C.c_size_t,
                         [C.c_void_p, C.c_void_p, C.POINTER(MBI), C.c_size_t])
        self.rpm = api("ReadProcessMemory", W.BOOL,
                       [C.c_void_p, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)])

    def processes(self):
        s = self.snapshot(2, 0)
        if not s or s == C.c_void_p(-1).value:
            raise C.WinError(C.get_last_error())
        out = []
        try:
            e = self.PE(); e.dwSize = C.sizeof(e)
            ok = self.pfirst(s, C.byref(e))
            while ok:
                if e.name.lower() in NAMES:
                    out.append(int(e.pid))
                ok = self.pnext(s, C.byref(e))
        finally:
            self.close(s)
        return out

    def verified_open(self, pid, root):
        h = self.open(0x0400 | 0x0010, False, pid)
        if not h:
            raise C.WinError(C.get_last_error())
        try:
            buf = C.create_unicode_buffer(32768); n = self.W.DWORD(len(buf))
            if not self.image(h, 0, buf, C.byref(n)):
                raise C.WinError(C.get_last_error())
            path = Path(buf.value)
            if path.name.lower() not in NAMES or os.path.normcase(os.path.realpath(path.parent)) != root:
                raise ValueError("Process executable is outside the exact allowed Steam installation")
            return h, str(path)
        except Exception:
            self.close(h)
            raise

    def modules(self, pid):
        s = self.snapshot(8 | 16, pid)
        if not s or s == C.c_void_p(-1).value:
            return []
        out = []
        try:
            e = self.ME(); e.dwSize = C.sizeof(e)
            ok = self.mfirst(s, C.byref(e))
            while ok:
                if e.name.lower() == "gameassembly.dll":
                    out.append({"path": e.path, "base": e.base, "size": e.size})
                ok = self.mnext(s, C.byref(e))
        finally:
            self.close(s)
        return out

    def read(self, handle, address, size):
        buf = C.create_string_buffer(size); count = C.c_size_t()
        C.set_last_error(0)
        ok = self.rpm(handle, address, buf, size, C.byref(count))
        return buf.raw[:min(count.value, size)], None if ok else C.get_last_error()

    def allocations(self, handle, deadline):
        groups = {}
        address = 0
        for _ in range(100000):
            if address >= 0x800000000000 or time.monotonic() >= deadline:
                break
            e = self.MBI()
            if not self.query(handle, address, C.byref(e), C.sizeof(e)):
                break
            start, size = int(e.base or 0), int(e.size)
            if not size or start + size <= address:
                break
            if e.allocationBase and e.type == 0x20000:
                base = int(e.allocationBase)
                g = groups.setdefault(base, {"base": base, "end": base, "regions": []})
                g["end"] = max(g["end"], start + size)
                g["regions"].append({"base": start, "size": size, "state": int(e.state),
                                     "protect": int(e.protect), "type": int(e.type)})
            address = start + size
        return groups.values()


def write_artifact(path, data, record):
    path.write_bytes(data)
    record.update({"file": path.name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    path.with_suffix(path.suffix + ".json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")


def capture(args):
    if os.name != "nt" or struct.calcsize("P") != 8:
        raise RuntimeError("Use a 64-bit Python on Windows; --self-test works on any OS")
    root_path = Path(args.steam_root).resolve()
    root = os.path.normcase(os.path.realpath(root_path))
    output = Path(args.output).resolve()
    if output == root_path or root_path in output.parents:
        raise ValueError("Output must be outside the installed game directory")
    ga = root_path / "GameAssembly.dll"
    with ga.open("rb") as f:
        disk_layout = pe_layout(f.read(65536))
    disk_hash = digest_file(ga)
    if disk_hash != args.expected_sha256.lower():
        raise ValueError("GameAssembly SHA-256 differs from the explicitly selected build")
    pid_file = Path(args.pid_file).resolve() if args.pid_file else None
    if pid_file is not None and pid_file.parent != output.parent:
        raise ValueError("PID file must be in the task-owned parent of the new capture directory")
    output.mkdir(parents=True, exist_ok=False)
    report = {"schemaVersion": 1, "startedUtc": utc(), "tool": Path(__file__).name,
              "toolSha256": digest_file(__file__), "status": "observing",
              "steamRoot": str(root_path), "expectedMetadataBytes": args.metadata_bytes,
              "gameAssemblyDiskSha256": disk_hash, "gameAssemblyDiskLayout": disk_layout,
              "access": "PROCESS_QUERY_INFORMATION | PROCESS_VM_READ",
              "mutatedTarget": False, "launchedTarget": False, "decoded": False,
              "candidateValidation": "heuristic only; offline table validation still required",
              "artifacts": [], "processes": [], "events": [],
              "observedModules": {}, "allocationDiagnostics": {},
              "pidBinding": {"mode": "pid_file" if pid_file else ("pid" if args.pid else "name_polling"),
                             "file": str(pid_file) if pid_file else None, "selectedPid": args.pid}}
    report_path = output / "capture-report.json"
    api = WindowsReader()
    deadline = time.monotonic() + args.seconds
    seen, errors, saved_meta, saved_ga = set(), set(), set(), set()
    ga_tries, meta_tries = collections.Counter(), collections.Counter()
    def event(kind, pid, detail):
        key = (kind, pid, str(detail))
        if key not in errors:
            errors.add(key); report["events"].append({"utc": utc(), "kind": kind, "pid": pid, "detail": detail})
    def flush():
        report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    flush()
    bound_pid = args.pid
    while time.monotonic() < deadline:
        if pid_file is not None and bound_pid is None and pid_file.exists():
            try:
                binding = json.loads(pid_file.read_text(encoding="utf-8"))
                candidate_pid = binding.get("pid")
                if type(candidate_pid) is not int or not 0 < candidate_pid < 2**32:
                    raise ValueError("PID file contains no valid process ID")
                if os.path.normcase(os.path.realpath(binding.get("captureOutput", ""))) != os.path.normcase(str(output)):
                    raise ValueError("PID file does not name this capture output directory")
                if binding.get("gameAssemblySha256") != disk_hash:
                    raise ValueError("PID file build hash does not match the selected build")
                expected_exe = Path(binding.get("executable", ""))
                if expected_exe.name.lower() not in NAMES or os.path.normcase(os.path.realpath(expected_exe.parent)) != root:
                    raise ValueError("PID file executable is outside the allowed installation")
                bound_pid = candidate_pid
                report["pidBinding"].update({"selectedPid": bound_pid, "boundUtc": utc()})
                flush()
            except (OSError, ValueError) as e:
                event("pid_binding_rejected", None, str(e))
                report["status"] = "pid_binding_rejected"
                break
        selected = [bound_pid] if bound_pid else ([] if pid_file else api.processes())
        for pid in selected:
            if time.monotonic() >= deadline:
                break
            try:
                handle, exe = api.verified_open(pid, root)
            except (OSError, ValueError) as e:
                event("process_unavailable_or_rejected", pid, str(e))
                continue
            try:
                if pid not in seen:
                    seen.add(pid); report["processes"].append({"pid": pid, "path": exe, "verifiedUtc": utc()})
                    print(json.dumps({"event": "verified_target", "pid": pid}), flush=True)
                reader = lambda address, size: api.read(handle, address, size)
                # Metadata first: its candidate allocations are far smaller than the mapped binary.
                allocations = list(api.allocations(handle, deadline))
                diagnostic_sizes = collections.Counter(a["end"] - a["base"] for a in allocations
                                                       if 1_000_000 <= a["end"] - a["base"] <= 100_000_000)
                report["allocationDiagnostics"][str(pid)] = {
                    "observedUtc": utc(), "privateAllocations": len(allocations),
                    "qualifyingSizeAllocations": sum(args.metadata_bytes <= a["end"] - a["base"] <= args.metadata_bytes + args.size_slack for a in allocations),
                    "nearestAllocationSizes": [{"bytes": n, "count": diagnostic_sizes[n]} for n in
                                               sorted(diagnostic_sizes, key=lambda n: abs(n - args.metadata_bytes))[:16]],
                    "note": "Sizes only; contents of unrelated-size allocations were not read."}
                for alloc in allocations:
                    size = alloc["end"] - alloc["base"]
                    key = (pid, alloc["base"])
                    if key in saved_meta or meta_tries[key] >= 2 or len(saved_meta) >= 8 or not args.metadata_bytes <= size <= args.metadata_bytes + args.size_slack:
                        continue
                    first = alloc["regions"][0]
                    if first["state"] != 0x1000 or first["protect"] & (0x100 | 0x01):
                        continue
                    header, _ = reader(alloc["base"], 256)
                    screen = header_candidate(header, args.metadata_bytes)
                    if not screen["plausible"]:
                        continue
                    meta_tries[key] += 1
                    started = utc()
                    data, coverage = read_span(reader, alloc["base"], args.metadata_bytes, deadline)
                    anchor = data.find(b"mscorlib.dll\0")
                    if anchor < 0:
                        event("allocation_rejected", pid, {"base": alloc["base"], "reason": "no_mscorlib_anchor", "coverage": coverage["complete"]})
                        continue
                    digest = hashlib.sha256(data).hexdigest()
                    name = f"pid-{pid}-metadata-candidate-{alloc['base']:x}-{digest[:12]}.bin"
                    record = {"kind": "metadata_candidate", "pid": pid, "processPath": exe,
                              "captureStartedUtc": started, "captureFinishedUtc": utc(),
                              "allocation": alloc, "headerScreen": screen, "anchorOffset": anchor,
                              "coverage": coverage, "decoded": False, "validatedTables": False}
                    write_artifact(output / name, data, record)
                    report["artifacts"].append(record)
                    if coverage["complete"]:
                        saved_meta.add(key)
                    print(json.dumps({"event": "metadata_candidate_saved", "pid": pid, "file": name,
                                      "complete": coverage["complete"], "decoded": False}), flush=True)
                    del data
                for module in api.modules(pid):
                    if os.path.normcase(os.path.realpath(module["path"])) != os.path.normcase(os.path.realpath(ga)):
                        event("module_rejected", pid, "GameAssembly path does not match selected disk file")
                        continue
                    key = (pid, module["base"])
                    if key in saved_ga or ga_tries[key] >= 3 or time.monotonic() >= deadline:
                        continue
                    if module["size"] != disk_layout["imageSize"]:
                        event("module_rejected", pid, "Module size differs from disk SizeOfImage")
                        continue
                    # Require a plausible entropy drop in BOTH protected code sections.
                    # This is only a capture trigger, not proof of correct decryption.
                    samples = []
                    with ga.open("rb") as source:
                        for section in disk_layout["sections"]:
                            if section["name"] not in (".text", "il2cpp"):
                                continue
                            n = min(65536, section["rawSize"], section["virtualSize"])
                            sample, err = reader(module["base"] + section["rva"], n)
                            source.seek(section["rawOffset"]); original = source.read(n)
                            delta = len(sample) == n and sample != original
                            runtime_entropy, disk_entropy = entropy(sample), entropy(original)
                            plausible = (n >= 4096 and len(sample) == n and delta
                                         and disk_entropy > 7.9 and 3.0 < runtime_entropy < 7.5)
                            samples.append({"section": section["name"], "sampleBytes": len(sample),
                                            "changedFromDisk": delta, "sampleEntropy": runtime_entropy,
                                            "diskSampleEntropy": disk_entropy, "entropyCandidate": plausible,
                                            "sampleSha256": hashlib.sha256(sample).hexdigest(), "error": err})
                    eligible = ({x["section"] for x in samples} == {".text", "il2cpp"}
                                and all(x["entropyCandidate"] for x in samples))
                    report["observedModules"][f"{pid}:{module['base']:x}"] = {
                        "observedUtc": utc(), "pid": pid, "module": module, "samples": samples,
                        "captureTriggerMet": eligible, "decryptedCodeVerified": False,
                        "note": "Only sample hashes/statistics retained until BOTH protected sections show a plausible entropy drop."}
                    if not eligible:
                        continue
                    ga_tries[key] += 1
                    started = utc()
                    data, coverage = read_span(reader, module["base"], module["size"], deadline)
                    name = f"pid-{pid}-GameAssembly-{module['base']:x}-attempt-{ga_tries[key]}.mapped.bin"
                    record = {"kind": "mapped_binary_evidence", "pid": pid, "processPath": exe,
                              "module": module, "captureStartedUtc": started, "captureFinishedUtc": utc(),
                              "samples": samples, "coverage": coverage, "decoded": False,
                              "decryptedCodeVerified": False,
                              "note": "Mapped layout; runtime data may contain resolved pointers. Not a rebuilt on-disk PE."}
                    write_artifact(output / name, data, record)
                    report["artifacts"].append(record)
                    if coverage["complete"]:
                        saved_ga.add(key)
                    print(json.dumps({"event": "module_evidence_saved", "pid": pid, "file": name,
                                      "complete": coverage["complete"], "decoded": False}), flush=True)
                    del data
            except (OSError, ValueError) as e:
                event("observation_error", pid, str(e))
            finally:
                api.close(handle)
            flush()
        if saved_meta and saved_ga:
            report["status"] = "candidate_evidence_captured"
            break
        time.sleep(min(0.1, max(0, deadline - time.monotonic())))
    else:
        report["status"] = "deadline_reached"
    report["finishedUtc"] = utc()
    report["completeMetadataCandidates"] = len(saved_meta)
    report["completeMappedImages"] = len(saved_ga)
    flush()
    print(json.dumps({"status": report["status"], "report": str(report_path),
                      "metadataCandidates": len(saved_meta), "mappedImages": len(saved_ga), "decoded": False}), flush=True)
    return 0 if saved_meta else 2


def self_test():
    import unittest
    class Tests(unittest.TestCase):
        def test_placeholder_rejected(self):
            self.assertFalse(header_candidate(b"R" * 256)["plausible"])
            self.assertFalse(header_candidate(b"\0" * 256)["plausible"])
        def test_permuted_header_screen(self):
            h = struct.pack("<64I", *([0x87654321, 0x12345678] + list(range(256, 318))))
            self.assertTrue(header_candidate(h)["plausible"])
        def test_complete_read(self):
            data = bytes(range(256)) * 100
            got, report = read_span(lambda p, n: (data[p:p+n], None), 0, len(data), time.monotonic() + 3)
            self.assertEqual(got, data); self.assertTrue(report["complete"])
            self.assertEqual(report["readBytes"], len(data))
        def test_holes_are_explicit(self):
            source = b"A" * PAGE + b"B" * PAGE + b"C" * PAGE
            def read(p, n):
                if p < PAGE and p + n > PAGE:
                    return source[p:PAGE], 299
                if PAGE <= p < 2 * PAGE:
                    return b"", 299
                return source[p:p+n], None
            got, report = read_span(read, 0, len(source), time.monotonic() + 3)
            self.assertFalse(report["complete"])
            self.assertEqual(report["unreadableRanges"], [{"offset": PAGE, "length": PAGE, "error": 299}])
            self.assertEqual(got[PAGE:2*PAGE], b"\0" * PAGE)
            self.assertEqual(report["readBytes"], 2 * PAGE)
        def test_deadline_marks_all_missing(self):
            data, report = read_span(lambda p, n: (b"x" * n, None), 0, 100, time.monotonic() - 1)
            self.assertFalse(report["complete"]); self.assertEqual(report["readBytes"], 0)
        def test_pe_bounds(self):
            data = bytearray(4096); data[:2] = b"MZ"
            struct.pack_into("<I", data, 0x3c, 0x80)
            data[0x80:0x84] = b"PE\0\0"
            struct.pack_into("<HH", data, 0x84, 0x8664, 1)
            struct.pack_into("<H", data, 0x94, 240)
            struct.pack_into("<H", data, 0x98, 0x20b)
            struct.pack_into("<Q", data, 0x98 + 24, 0x180000000)
            struct.pack_into("<II", data, 0x98 + 56, 8192, 512)
            at = 0x98 + 240; data[at:at+8] = b".text\0\0\0"
            struct.pack_into("<IIII", data, at+8, 512, 4096, 512, 512)
            result = pe_layout(data); self.assertEqual(result["imageSize"], 8192)
            struct.pack_into("<I", data, at+8, 100000)
            with self.assertRaises(ValueError): pe_layout(data)
        def test_entropy(self):
            self.assertEqual(entropy(b"R" * 100), 0)
            self.assertEqual(entropy(bytes(range(256))), 8)
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Tests)
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", help="New task-owned output directory outside Steam install")
    p.add_argument("--steam-root", default=STEAM_ROOT)
    selection = p.add_mutually_exclusive_group()
    selection.add_argument("--pid", type=int, help="Exact PID; otherwise poll the two allowed Rec Room names")
    selection.add_argument("--pid-file", help="Task-owned JSON binding written by the recovery wrapper after its own launch")
    p.add_argument("--seconds", type=float, default=45)
    p.add_argument("--metadata-bytes", type=int, default=METADATA_BYTES)
    p.add_argument("--size-slack", type=int, default=128 * 1024)
    p.add_argument("--expected-sha256", default=GAMEASSEMBLY_SHA256)
    p.add_argument("--self-test", action="store_true")
    args = p.parse_args()
    if args.self_test:
        return self_test()
    if not args.output:
        p.error("--output is required")
    if not 1 <= args.seconds <= 300 or not 1_000_000 <= args.metadata_bytes <= 100_000_000 or not 0 <= args.size_slack <= 1024 * 1024:
        p.error("Use 1..300 seconds, 1..100 MB metadata, and <=1 MiB size slack")
    return capture(args)


if __name__ == "__main__":
    sys.exit(main())
