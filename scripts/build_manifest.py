#!/usr/bin/env python3
"""Create the desktop package's SHA256 manifest without changing its payload."""
import argparse
import datetime
import hashlib
import json
import pathlib
import subprocess


def git_value(root, *arguments):
    try:
        result = subprocess.run(
            ["git", "-C", str(root), *arguments],
            check=True, capture_output=True, text=True, timeout=10,
        )
        return result.stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_directory", type=pathlib.Path)
    parser.add_argument("--runtime", choices=("linux-x64", "win-x64"), required=True)
    parser.add_argument("--sdk", required=True)
    parser.add_argument("--source-root", type=pathlib.Path, required=True)
    options = parser.parse_args()
    output = options.output_directory.resolve(strict=True)
    if not output.is_dir():
        parser.error("output_directory must be a directory")
    manifest = output / "build-manifest.json"
    if manifest.exists():
        parser.error("a build manifest already exists; refusing to overwrite release evidence")
    files = []
    for path in sorted(output.rglob("*")):
        if path.is_symlink():
            parser.error(f"package symlink requires explicit release handling: {path.relative_to(output)}")
        if path.is_file():
            with path.open("rb") as source:
                digest = hashlib.file_digest(source, "sha256").hexdigest()
            files.append({"path": path.relative_to(output).as_posix(), "bytes": path.stat().st_size, "sha256": digest})
    source_root = options.source_root.resolve(strict=True)
    commit = git_value(source_root, "rev-parse", "HEAD") if (source_root / ".git").exists() else None
    status = git_value(source_root, "status", "--porcelain") if commit else None
    record = {
        "schemaVersion": 1,
        "product": "Metallic-RC",
        "runtime": options.runtime,
        "sdk": options.sdk,
        "builtAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "sourceCommit": commit,
        "sourceTreeDirty": bool(status) if status is not None else None,
        "files": files,
    }
    with manifest.open("x", encoding="utf-8") as destination:
        json.dump(record, destination, indent=2, ensure_ascii=False)
        destination.write("\n")
    print(f"Package manifest: {len(files)} files, {manifest}")


if __name__ == "__main__":
    main()
