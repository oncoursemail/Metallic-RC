#!/usr/bin/env python3
"""Inspect a runtime metadata candidate; never repairs or emits a decoded file.

An exact-length tiling is necessary but not sufficient. All mappings below remain
candidate roles until cross-table constraints and an independent parser pass.
Format reference: Perfare/Il2CppDumper MetadataClass.cs (MIT); provenance is in
metadata-primary-sources.json. No build-specific table offsets are assumed.
"""
import argparse
import collections
import hashlib
import json
import pathlib
import struct
import time


def solve_header(data, length, table_count=31, max_solutions=32,
                 max_states=1_000_000, seconds=20.0):
    if length < 256 or length > len(data):
        raise ValueError("exact length is outside captured bytes")
    vals = struct.unpack_from("<64I", data)
    counts = collections.Counter(vals)
    states = 0
    stopped = None
    solutions = []
    seen = set()
    deadline = time.monotonic() + seconds
    sizes = sorted(counts)

    def visit(cur, chain):
        nonlocal states, stopped
        if stopped:
            return
        states += 1
        if states > max_states:
            stopped = "state_limit"
            return
        if states % 1024 == 0 and time.monotonic() >= deadline:
            stopped = "time_limit"
            return
        if len(chain) == table_count:
            if cur == length:
                key = tuple(chain)
                if key not in seen:
                    seen.add(key)
                    solutions.append({
                        "regions": [{"index": i, "offset": o, "size": s}
                                    for i, (o, s) in enumerate(chain)],
                        "unusedHeaderValues": sorted(counts.elements()),
                        "end": cur,
                    })
                    if len(solutions) >= max_solutions:
                        stopped = "solution_limit"
            return
        if not counts[cur]:
            return
        counts[cur] -= 1
        last = len(chain) + 1 == table_count
        for size in sizes:
            if not counts[size] or cur + size > length:
                continue
            counts[size] -= 1
            nxt = cur + size
            if (last and nxt == length) or (not last and counts[nxt]):
                visit(nxt, chain + [(cur, size)])
            counts[size] += 1
            if stopped:
                break
        counts[cur] += 1

    visit(256, [])
    return {"solutions": solutions, "states": states,
            "searchComplete": stopped is None, "stoppedReason": stopped,
            "exactLength": length, "tableCount": table_count,
            "headerDistinctValues": len(set(vals))}


def region_bytes(data, r):
    return memoryview(data)[r["offset"]:r["offset"] + r["size"]]


def str_at(data, blob, idx, empty=False):
    if not 0 <= idx < blob["size"]:
        return None
    pos = blob["offset"] + idx
    end = data.find(b"\0", pos, min(blob["offset"] + blob["size"], pos + 4097))
    if end < 0 or (end == pos and not empty):
        return None
    try:
        value = data[pos:end].decode("utf-8")
    except UnicodeDecodeError:
        return None
    if any(ord(c) < 32 for c in value):
        return None
    return value


def record_candidates(data, regions, blob):
    # v31: (stride, token byte offset, token table identifier, name byte offset)
    specs = {
        "methods": (36, 24, 0x06, 0),
        "fields": (12, 8, 0x04, 0),
        "parameters": (12, 4, 0x08, 0),
        "properties": (20, 16, 0x17, 0),
        "events": (24, 20, 0x14, 0),
        "typeDefinitions": (88, 84, 0x02, 0),
        "assemblies": (64, 4, 0x20, 16),
    }
    result = {name: [] for name in specs}
    result["images"] = []
    for r in regions:
        for role, (stride, tokoff, token_table, nameoff) in specs.items():
            if not r["size"] or r["size"] % stride:
                continue
            n = r["size"] // stride
            def token_ok(i):
                tok = struct.unpack_from("<I", data, r["offset"] + i*stride + tokoff)[0]
                return tok >> 24 == token_table and (tok & 0xFFFFFF) != 0
            sample = sorted(set(range(0, n, max(1, n//97))) | {n-1})
            if not all(token_ok(i) for i in sample):
                continue
            bad_tokens = bad_names = bad_namespace = 0
            for i in range(n):
                p = r["offset"] + i*stride
                bad_tokens += not token_ok(i)
                name = struct.unpack_from("<I", data, p + nameoff)[0]
                bad_names += str_at(data, blob, name) is None
                if role == "typeDefinitions":
                    ns = struct.unpack_from("<I", data, p + 4)[0]
                    bad_namespace += str_at(data, blob, ns, empty=True) is None
            # Corrupt type names are reported explicitly, never repaired here.
            if not bad_tokens and not bad_namespace and (
                not bad_names or role == "typeDefinitions"
            ):
                result[role].append({"region": r["index"], "stride": stride,
                    "count": n, "allTokenTagsChecked": True,
                    "badNames": bad_names, "badNamespaces": bad_namespace,
                    "status": "structural_candidate"})
        if r["size"] and r["size"] % 40 == 0:
            n = r["size"] // 40
            valid = True
            for i in range(n):
                vals = struct.unpack_from("<10I", data, r["offset"] + i*40)
                name = str_at(data, blob, vals[0])
                if name is None or not name.lower().endswith(".dll") or vals[1] >= n:
                    valid = False
                    break
                if vals[7] >> 24 != 0 or vals[7] == 0:
                    valid = False
                    break
            if valid:
                result["images"].append({"region": r["index"], "stride": 40,
                    "count": n, "allNamesAndTokensChecked": True,
                    "status": "structural_candidate"})
    return result


def derived_constraints(data, regions, roles):
    by_id = {r["index"]: r for r in regions}
    counts = {}
    violations = []
    if len(roles.get("typeDefinitions", [])) == 1:
        r = by_id[roles["typeDefinitions"][0]["region"]]
        # Named table, start dword, ushort count index, stride.
        fields = [("fields",8,2,12),("methods",9,0,36),
            ("events",10,3,24),("properties",11,1,20),
            ("nestedTypes",12,4,4),("interfaces",13,6,4),
            ("vtableMethods",14,5,4),("interfaceOffsets",15,7,8)]
        maxima = {name: 0 for name, _, _, _ in fields}
        for i in range(r["size"] // 88):
            p = r["offset"] + i*88
            starts = struct.unpack_from("<16i", data, p)
            lens = struct.unpack_from("<8H", data, p+64)
            for name, si, ci, stride in fields:
                s, n = starts[si], lens[ci]
                if s < -1 or (s < 0 and n):
                    if len(violations) < 25:
                        violations.append({"typeIndex":i,"table":name,
                            "start":s,"count":n})
                if s >= 0:
                    maxima[name] = max(maxima[name], s+n)
        for name, _, _, stride in fields:
            n = maxima[name]
            counts[name] = {"minimumCount": n, "stride": stride,
                "exactSizeCandidateRegions": [q["index"] for q in regions if q["size"] == n*stride]}
    if len(roles.get("methods", [])) == 1:
        r = by_id[roles["methods"][0]["region"]]
        end = 0
        for i in range(r["size"] // 36):
            p = r["offset"] + i*36
            start = struct.unpack_from("<i", data, p+16)[0]
            n = struct.unpack_from("<H", data, p+34)[0]
            if start < -1 or (start < 0 and n):
                if len(violations) < 25:
                    violations.append({"methodIndex":i,"table":"parameters",
                        "start":start,"count":n})
            if start >= 0:
                end = max(end, start+n)
        counts["parameters"] = {"minimumCount": end, "stride":12,
            "exactSizeCandidateRegions": [q["index"] for q in regions if q["size"] == end*12]}
    return {"derivedCountConstraints": counts, "invalidRangesFirst25": violations,
            "note":"Start+count maxima are lower bounds until partition coverage is proved."}


def literal_candidates(data, regions):
    found = []
    for r in regions:
        if not r["size"] or r["size"] % 8:
            continue
        cursor, valid = 0, True
        for size, index in struct.iter_unpack("<II", region_bytes(data, r)):
            if index != cursor or size > len(data) or cursor+size > len(data):
                valid = False
                break
            cursor += size
        if valid and cursor > 0:
            matches = [q["index"] for q in regions if q["index"] != r["index"] and q["size"] == cursor]
            if matches:
                found.append({"stringLiteralRegion":r["index"],
                    "literalCount":r["size"]//8,"literalDataRegionCandidates":matches,
                    "dataBytes":cursor,"allEntriesTileBlobExactly":True})
    return found


def inspect(data, length, **solver_options):
    report = {"status":"unvalidated_runtime_candidate", "decoded":False,
        "inputBytes":len(data), "exactLength":length,
        "sha256":hashlib.sha256(data).hexdigest(),
        "bodySha256":hashlib.sha256(data[256:length]).hexdigest(),
        "notes":["No repairs or output metadata emitted.",
                 "Structural candidates require cross-table identification and independent parser validation."]}
    if length >= 1 and data[:length].count(0x52) == length:
        report.update(status="uniform_R_placeholder",reason="No metadata tables exist in these bytes.")
        return report
    tiling = solve_header(data, length, **solver_options)
    for solution in tiling["solutions"]:
        regions = solution["regions"]
        blobs = [r for r in regions if
            data.find(b"mscorlib.dll\0", r["offset"], r["offset"]+r["size"]) >= 0 and
            data.find(b"Assembly-CSharp.dll\0", r["offset"], r["offset"]+r["size"]) >= 0]
        solution["identifierBlobCandidates"] = [r["index"] for r in blobs]
        solution["literalCandidates"] = literal_candidates(data, regions)
        if len(blobs) == 1:
            roles = record_candidates(data, regions, blobs[0])
            solution["recordRoleCandidates"] = roles
            solution.update(derived_constraints(data, regions, roles))
        solution["unresolvedRoles"] = [
            "All 31 roles must be identified and cross-validated before canonical rebuild."
        ]
    report["headerSearch"] = tiling
    return report


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("input", type=pathlib.Path)
    ap.add_argument("--length", type=int, default=52_403_096)
    ap.add_argument("--output", type=pathlib.Path, required=True)
    ap.add_argument("--max-solutions", type=int, default=32)
    ap.add_argument("--max-states", type=int, default=1_000_000)
    ap.add_argument("--seconds", type=float, default=20.0)
    args = ap.parse_args()
    if args.output.exists():
        ap.error("output exists; choose a new evidence filename")
    if not 256 <= args.length <= args.input.stat().st_size <= 128*1024*1024:
        ap.error("input/exact length is outside the bounded 128 MiB metadata profile")
    data = args.input.read_bytes()
    report = inspect(data,args.length,max_solutions=args.max_solutions,
        max_states=args.max_states,seconds=args.seconds)
    report["inputPath"] = str(args.input.resolve())
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"status":report["status"],"decoded":False,
        "tilings":len(report.get("headerSearch",{}).get("solutions",[])),
        "output":str(args.output.resolve())}))


if __name__ == "__main__":
    main()
