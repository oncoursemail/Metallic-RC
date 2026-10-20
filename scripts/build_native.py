#!/usr/bin/env python3
"""Build a versioned private Metallic-RC playtest without changing the source tree.

Supply the official Godot 4.5.2 editor and extracted export templates. This script
does not download executables, sign a release, publish, or change a running game.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PROJECT = ROOT / "game" / "native"
VERSION = "4.5.2.stable.official.6ce3de25a"
TARGETS = {
    "linux": ("Linux", "Metallic-RC.x86_64", "linux_release.x86_64"),
    "windows": ("Windows", "Metallic-RC.exe", "windows_release_x86_64.exe"),
    "server": ("Linux Server", "Metallic-RC-Server.x86_64", "linux_release.x86_64"),
}


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source_revision() -> tuple[str, bool]:
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=normal"], cwd=ROOT, text=True
    ).strip())
    return commit, dirty


def source_digest(snapshot: Path) -> str:
    """Hash every staged source file, before import or local template changes."""
    digest = hashlib.sha256()
    for path in sorted(snapshot.rglob("*"), key=lambda item: item.relative_to(snapshot).as_posix()):
        if path.is_symlink():
            raise RuntimeError(f"Source symlinks are not supported: {path.relative_to(snapshot)}")
        if path.is_dir():
            continue
        if not path.is_file():
            raise RuntimeError(f"Unsupported source file: {path.relative_to(snapshot)}")
        digest.update(path.relative_to(snapshot).as_posix().encode() + b"\0")
        digest.update(bytes.fromhex(sha256(path)))
    return digest.hexdigest()


def stage_sources(stage: Path, commit: str, dirty: bool) -> None:
    if dirty:
        if PROJECT.is_symlink():
            raise RuntimeError("The native project must not be a symlink")
        # Preserve links while copying so validation rejects them without ever
        # following them outside the project. Hash this completed snapshot only.
        shutil.copytree(PROJECT, stage, symlinks=True,
                        ignore=shutil.ignore_patterns(".godot", "*.tmp"))
        return
    stage.mkdir()
    # A clean build is pinned to the recorded commit even if another checkout
    # edit or commit occurs while this process is exporting.
    with tempfile.TemporaryFile() as stream:
        subprocess.run(["git", "archive", "--format=tar", f"{commit}:game/native"],
                       cwd=ROOT, stdout=stream, check=True, timeout=30)
        stream.seek(0)
        with tarfile.open(fileobj=stream, mode="r:") as archive:
            for member in archive:
                relative = PurePosixPath(member.name)
                if (relative.is_absolute() or not relative.parts or ".." in relative.parts
                        or "\\" in member.name or ":" in member.name):
                    raise RuntimeError(f"Unsafe source archive path: {member.name}")
                destination = stage.joinpath(*relative.parts)
                if member.isdir():
                    destination.mkdir(parents=True, exist_ok=True)
                elif member.isfile():
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with archive.extractfile(member) as source, destination.open("xb") as target:
                        shutil.copyfileobj(source, target)
                else:
                    raise RuntimeError(f"Unsupported source archive entry: {member.name}")


def check_export_log(log_path: Path) -> None:
    log_text = log_path.read_text(encoding="utf-8", errors="replace")
    if any(marker in log_text for marker in ("SCRIPT ERROR:", "Parse Error:", "Failed to load script")):
        raise RuntimeError(f"Godot reported a script error; review {log_path}. No packages were published.")


def write_zip(folder: Path, archive: Path, executable: str) -> None:
    # Sorted paths and fixed ZIP timestamps make the packaging step repeatable.
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as output:
        for path in sorted(folder.rglob("*")):
            if not path.is_file():
                continue
            info = zipfile.ZipInfo(path.relative_to(folder.parent).as_posix(), (2026, 10, 20, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            # Host access checks are not portable (especially on Windows).
            info.external_attr = (0o100755 if path.name == executable else 0o100644) << 16
            output.writestr(info, path.read_bytes())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--godot", type=Path, required=True)
    parser.add_argument("--templates", type=Path, required=True,
                        help="Directory containing linux_release.x86_64 and windows_release_x86_64.exe")
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts" / "native")
    parser.add_argument("--target", choices=["all", *TARGETS], default="all")
    parser.add_argument("--allow-dirty", action="store_true", help="Explicitly label an uncommitted developer build")
    args = parser.parse_args()
    godot = args.godot.resolve(strict=True)
    templates = args.templates.resolve(strict=True)
    output = args.output.resolve()
    if output == PROJECT or PROJECT in output.parents:
        parser.error("Output must be outside the native source directory")
    version = subprocess.check_output([str(godot), "--headless", "--version"], text=True).strip()
    if version != VERSION:
        parser.error(f"Expected {VERSION}; received {version}")
    commit, dirty = source_revision()
    if dirty and not args.allow_dirty:
        parser.error("Commit source first, or explicitly pass --allow-dirty for an uncommitted developer build")
    targets = TARGETS if args.target == "all" else {args.target: TARGETS[args.target]}
    for _, _, template in targets.values():
        if not (templates / template).is_file():
            parser.error(f"Missing export template: {template}")
    try:
        output.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        parser.error("Output already exists. Choose a fresh path to preserve earlier build receipts and logs.")

    # Packages stay private to staging until every export passes its log checks.
    # Failed exports leave only export.log, not success-looking BUILD.json/ZIPs.
    with tempfile.TemporaryDirectory(prefix="metallic-build-", dir=output) as temp:
        stage = Path(temp) / "project"
        stage_sources(stage, commit, dirty)
        manifest = {
            "schema": "metallic-rc-playtest-build-v1", "project": "Metallic-RC",
            "version": "0.1.0-playtest.1", "commit": commit,
            "source_dirty": dirty, "source_sha256": source_digest(stage), "godot": version,
            "status": "private-development-build", "signed": False,
            "verification": "See the separate test receipt for this exact commit and artifact digest.",
            "artifacts": {},
        }
        template_stage = Path(temp) / "templates"
        template_stage.mkdir()
        for template_name in sorted({item[2] for item in targets.values()}):
            shutil.copyfile(templates / template_name, template_stage / template_name)
        packages = Path(temp) / "packages"
        packages.mkdir()
        preset = stage / "export_presets.cfg"
        sections = preset.read_text(encoding="utf-8").split("[preset.")
        patched = []
        for section in sections:
            if section.startswith("1.options]"):
                template_path = template_stage / "windows_release_x86_64.exe"
            elif section.startswith(("0.options]", "2.options]")):
                template_path = template_stage / "linux_release.x86_64"
            else:
                patched.append(section)
                continue
            section = section.replace('custom_template/release=""',
                                      "custom_template/release=" + json.dumps(template_path.as_posix()))
            patched.append(section)
        preset.write_text("[preset.".join(patched), encoding="utf-8", newline="\n")
        log_path = output / "export.log"
        with log_path.open("w", encoding="utf-8") as log:
            base = [str(godot), "--headless", "--path", str(stage)]
            build_env = os.environ.copy()
            if os.name != "nt":
                for key, folder_name in (("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")):
                    build_env[key] = str(Path(temp) / folder_name)
            subprocess.run(base + ["--editor", "--import", "--quit"],
                           stdout=log, stderr=subprocess.STDOUT, check=True, timeout=120, env=build_env)
            log.flush()
            check_export_log(log_path)
            for target, (name, binary, template) in targets.items():
                folder = packages / target
                folder.mkdir()
                subprocess.run(base + ["--export-release", name, str(folder / binary)],
                               stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180, env=build_env)
                log.flush()
                check_export_log(log_path)
                if not (folder / binary).is_file() or not (folder / (Path(binary).stem + ".pck")).is_file():
                    raise RuntimeError(f"Exporter did not produce executable and pack for {target}")
                for license_name in ("THIRD_PARTY_NOTICES.txt", "GODOT_LICENSE.txt"):
                    license_path = stage / license_name
                    if not license_path.is_file():
                        raise RuntimeError(f"Missing redistribution notice: {license_name}")
                    shutil.copyfile(license_path, folder / license_name)
                quickstart = (
                    "METALLIC-RC · METALLIC MEADOW\nPrivate native playtest 0.1.0-playtest.1\n\n"
                    "Keep the executable and .pck together. The pack contains the game.\n"
                    "Desktop: open the executable, enter a name, then choose Host meadow.\n"
                    "For a second player on the same computer, open a second instance, enter address 127.0.0.1 and port 27841 in the separate fields, then choose Join address.\n"
                    "Controls: WASD/arrow keys to move, mouse to look, Space to jump, E to interact, Tab for the watch, Esc for menu.\n"
                    "Visit the three Meadow landmarks, then ring the community bell in the barn.\n\n"
                    "A server is private by default and binds only to loopback.\n"
                    "Internet hosting, account login, voice, VR, creator publishing and the marketplace are not included in this playtest.\n"
                    "This build is unsigned; see BUILD.json for source revision and integrity hashes.\n"
                )
                if target == "server":
                    quickstart += "\nStart: ./Metallic-RC-Server.x86_64 --headless -- --server --bind 127.0.0.1 --port 27841 --state-file /absolute/private/path/meadow.json\n"
                (folder / "README.txt").write_text(quickstart, encoding="utf-8", newline="\n")
                for path in folder.iterdir():
                    if path.is_file():
                        path.chmod(0o755 if path.name == binary else 0o644)
                receipt = {key: value for key, value in manifest.items() if key != "artifacts"}
                receipt["target"] = target
                receipt["template_sha256"] = sha256(template_stage / template)
                receipt["files"] = {
                    path.name: {"sha256": sha256(path), "bytes": path.stat().st_size}
                    for path in sorted(folder.iterdir()) if path.is_file()
                }
                (folder / "BUILD.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8", newline="\n")
                archive = packages / f"Metallic-RC-0.1.0-playtest.1-{target}.zip"
                write_zip(folder, archive, binary)
                manifest["artifacts"][target] = {"file": archive.name, "sha256": sha256(archive), "bytes": archive.stat().st_size}
        for path in packages.iterdir():
            path.replace(output / path.name)
        (output / "build-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
    for target, artifact in manifest["artifacts"].items():
        print(f"Built {target}: {artifact['file']}", flush=True)
    print(json.dumps({"commit": commit, "source_dirty": dirty, "output": str(output)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
