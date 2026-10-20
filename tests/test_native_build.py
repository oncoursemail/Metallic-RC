"""Build provenance and failed-export checks; fixture exports are not Godot QA."""
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest import mock
import zipfile


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("metallic_native_build", ROOT / "scripts/build_native.py")
build = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(build)


class NativeBuildTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.project = self.root / "game/native"
        self.project.mkdir(parents=True)
        (self.project / "project.godot").write_text("committed-source\n")
        (self.project / "export_presets.cfg").write_text(
            (ROOT / "game/native/export_presets.cfg").read_text()
        )
        for name in ("GODOT_LICENSE.txt", "THIRD_PARTY_NOTICES.txt"):
            (self.project / name).write_text("original-notice\n")
        self.godot = self.root / "godot"
        self.godot.write_bytes(b"fixture-not-a-real-engine")
        self.templates = self.root / "templates"
        self.templates.mkdir()
        for name in ("linux_release.x86_64", "windows_release_x86_64.exe"):
            (self.templates / name).write_bytes(b"original-template")
        self.output = self.root / "output"
        self.addCleanup(mock.patch.stopall)
        mock.patch.object(build, "ROOT", self.root).start()
        mock.patch.object(build, "PROJECT", self.project).start()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True, stderr=subprocess.PIPE).strip()

    def commit(self):
        self.git("-c", "init.defaultBranch=main", "init")
        self.git("add", "game/native")
        self.git("-c", "user.name=Build Test", "-c", "user.email=build@example.invalid",
                 "commit", "-m", "fixture")
        return self.git("rev-parse", "HEAD")

    def run_fixture(self, *, target="server", fail_target=None, mutate_live=False):
        def fake_export(command, **kwargs):
            if "--import" in command:
                if mutate_live:
                    (self.project / "project.godot").write_text("edited-after-staging\n")
                    (self.project / "GODOT_LICENSE.txt").write_text("edited-notice\n")
                    (self.templates / "linux_release.x86_64").write_bytes(b"edited-template")
            elif "--export-release" in command:
                binary = Path(command[-1])
                stage = Path(command[command.index("--path") + 1])
                self.assertEqual((stage / "project.godot").read_text(), "committed-source\n")
                binary.write_bytes(b"fixture-executable")
                binary.with_suffix(".pck").write_bytes(b"fixture-game-pack")
                if command[-2] == fail_target:
                    kwargs["stdout"].write("SCRIPT ERROR: fixture parse failure\n")
            else:
                self.fail(f"Unexpected child command: {command}")
            return subprocess.CompletedProcess(command, 0)

        argv = ["build_native.py", "--godot", str(self.godot), "--templates", str(self.templates),
                "--output", str(self.output), "--target", target, "--allow-dirty"]
        with (mock.patch("sys.argv", argv),
              mock.patch.object(build, "source_revision", return_value=("a" * 40, True)),
              mock.patch.object(build.subprocess, "check_output", return_value=build.VERSION),
              mock.patch.object(build.subprocess, "run", side_effect=fake_export),
              contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO())):
            return build.main()

    def test_clean_stage_uses_exact_commit_after_worktree_changes(self):
        commit = self.commit()
        (self.project / "project.godot").write_text("later-worktree-edit\n")
        (self.project / "untracked.gd").write_text("must-not-be-in-clean-export\n")
        stage = self.root / "snapshot"
        build.stage_sources(stage, commit, False)
        self.assertEqual((stage / "project.godot").read_text(), "committed-source\n")
        self.assertFalse((stage / "untracked.gd").exists())

    def test_dirty_stage_digest_covers_snapshot_including_tests(self):
        (self.project / "tests").mkdir()
        (self.project / "tests/test.gd").write_text("test-input\n")
        (self.project / ".godot").mkdir()
        (self.project / ".godot/cache").write_text("ignored-cache\n")
        (self.project / "editor.tmp").write_text("ignored-temporary\n")
        stage = self.root / "snapshot"
        build.stage_sources(stage, "a" * 40, True)
        before = build.source_digest(stage)
        (self.project / "project.godot").write_text("later-worktree-edit\n")
        self.assertEqual(build.source_digest(stage), before)
        self.assertFalse((stage / ".godot").exists())
        self.assertFalse((stage / "editor.tmp").exists())
        (stage / "tests/test.gd").write_text("changed-staged-test\n")
        self.assertNotEqual(build.source_digest(stage), before)

    def test_symlink_source_is_rejected_without_following_it(self):
        outside = self.root / "outside.txt"
        outside.write_text("must-not-be-copied\n")
        (self.project / "linked.txt").symlink_to(outside)
        stage = self.root / "snapshot"
        build.stage_sources(stage, "a" * 40, True)
        self.assertTrue((stage / "linked.txt").is_symlink())
        with self.assertRaisesRegex(RuntimeError, "symlinks"):
            build.source_digest(stage)
        self.assertEqual(outside.read_text(), "must-not-be-copied\n")

    def test_clean_archive_rejects_traversal_and_links(self):
        for name, kind in (("../outside.txt", tarfile.REGTYPE), ("linked", tarfile.SYMTYPE)):
            with self.subTest(name=name), tempfile.TemporaryDirectory(dir=self.root) as temp:
                payload = io.BytesIO()
                with tarfile.open(fileobj=payload, mode="w") as archive:
                    member = tarfile.TarInfo(name)
                    member.type = kind
                    member.linkname = "../outside.txt" if kind == tarfile.SYMTYPE else ""
                    archive.addfile(member)

                def write_archive(*args, **kwargs):
                    kwargs["stdout"].write(payload.getvalue())
                    return subprocess.CompletedProcess(args[0], 0)

                with mock.patch.object(build.subprocess, "run", side_effect=write_archive):
                    with self.assertRaises(RuntimeError):
                        build.stage_sources(Path(temp) / "snapshot", "a" * 40, False)
        self.assertFalse((self.root / "outside.txt").exists())

    def test_receipt_and_notices_use_staged_source_and_template(self):
        expected_source = build.source_digest(self.project)
        self.assertEqual(self.run_fixture(mutate_live=True), 0)
        folder = self.output / "server"
        receipt = json.loads((folder / "BUILD.json").read_text())
        self.assertEqual(receipt["source_sha256"], expected_source)
        self.assertNotEqual(receipt["source_sha256"], build.source_digest(self.project))
        self.assertEqual(receipt["template_sha256"], hashlib.sha256(b"original-template").hexdigest())
        self.assertEqual((folder / "GODOT_LICENSE.txt").read_text(), "original-notice\n")
        for name, entry in receipt["files"].items():
            content = (folder / name).read_bytes()
            self.assertEqual(hashlib.sha256(content).hexdigest(), entry["sha256"])
            self.assertEqual(len(content), entry["bytes"])
        readme = (folder / "README.txt").read_bytes()
        self.assertIn(b"Host meadow", readme)
        self.assertIn(b"Join address", readme)
        self.assertIn(b"separate fields", readme)
        self.assertNotIn(b"\r", readme)
        manifest = json.loads((self.output / "build-manifest.json").read_text())
        archive = manifest["artifacts"]["server"]
        self.assertEqual(build.sha256(self.output / archive["file"]), archive["sha256"])

    def test_late_script_error_never_publishes_earlier_target_packages(self):
        with self.assertRaisesRegex(RuntimeError, "No packages were published"):
            self.run_fixture(target="all", fail_target="Windows")
        self.assertEqual([path.name for path in self.output.iterdir()], ["export.log"])
        self.assertIn("fixture parse failure", (self.output / "export.log").read_text())

    def test_existing_output_receipts_are_never_overwritten(self):
        self.output.mkdir()
        receipt = self.output / "build-manifest.json"
        receipt.write_text("previous-build\n")
        with self.assertRaises(SystemExit):
            self.run_fixture()
        self.assertEqual(receipt.read_text(), "previous-build\n")
        self.assertEqual(len(list(self.output.iterdir())), 1)

    def test_output_inside_source_is_rejected_before_staging(self):
        self.output = self.project / "recursive-output"
        with self.assertRaises(SystemExit):
            self.run_fixture()
        self.assertFalse(self.output.exists())

    def test_zip_modes_and_timestamps_do_not_depend_on_host_permissions(self):
        package = self.root / "server"
        package.mkdir()
        for name in ("Metallic-RC-Server.x86_64", "Metallic-RC-Server.pck", "README.txt"):
            path = package / name
            path.write_bytes(name.encode())
            path.chmod(0o777)
        first, second = self.root / "one.zip", self.root / "two.zip"
        build.write_zip(package, first, "Metallic-RC-Server.x86_64")
        for path in package.iterdir():
            path.chmod(0o600)
        build.write_zip(package, second, "Metallic-RC-Server.x86_64")
        self.assertEqual(build.sha256(first), build.sha256(second))
        with zipfile.ZipFile(first) as archive:
            for entry in archive.infolist():
                expected = 0o755 if entry.filename.endswith(".x86_64") else 0o644
                self.assertEqual((entry.external_attr >> 16) & 0o777, expected)
                self.assertEqual(entry.date_time, (2026, 10, 20, 0, 0, 0))


if __name__ == "__main__":
    unittest.main()
