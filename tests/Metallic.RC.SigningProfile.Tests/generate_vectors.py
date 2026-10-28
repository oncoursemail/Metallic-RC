"""Synthetic formula oracle using Python stdlib, not native-client execution.

Run this script to stdout and compare with vectors.json; it writes no files.
It deliberately assembles the signed message before the HMAC operation rather
than calling the C# implementation or its incremental-hash code path.
"""
import base64
import hashlib
import hmac
import json
import struct
import sys
from pathlib import Path

KEY = bytes(range(32))
PATH = "/api/v1/synthetic?x=1&name=unit%20test"
rows = []
for size in [None, 0, 2048, 2049, 2063, 2064, 4096, 65536]:
    message = PATH.encode("ascii")
    if size:
        body = bytes((index * 37 + 11) % 256 for index in range(size))
        if size <= 2048:
            selected = body
        else:
            offsets = range(0, 16 * (size // 16), size // 16)
            selected = b"".join(body[offset:offset + 128] for offset in offsets)
        message += struct.pack("<i", size) + selected
    rows.append({"bodyLength": size, "signature": base64.b64encode(hmac.digest(KEY, message, "sha256")).decode("ascii"), "messageSha256": hashlib.sha256(message).hexdigest()})
if sys.argv[1:] == ["--check"]:
    saved = json.loads(Path(__file__).with_name("vectors.json").read_text(encoding="utf-8-sig"))
    if saved != rows:
        raise SystemExit("Synthetic signing vectors differ from Python oracle.")
    print("8 synthetic signing vectors match Python stdlib oracle.")
elif not sys.argv[1:]:
    print(json.dumps(rows, indent=2))
else:
    raise SystemExit("Use [--check].")
