# Compiled-client rework validation

Recorded 20 October 2026 UTC. This is development-tool acceptance, not a Rec Room game release. It records what was run and preserves the remaining game and platform gates.

## Implemented and tested

The new C# solution contains a desktop launcher, read-only client inventory and registered-file verification, an original typed circuit compiler/preview, and a loopback creator website with persistent room drafts. The service exposes explicit capability flags; game protocol integration, multiplayer, voice and account migration remain false.

| Check | Observed result | Scope |
| --- | --- | --- |
| Final Linux package build | 18/18 Core checks and 103 Platform/Creator assertions passed | Pinned .NET SDK 10.0.402; frozen desktop input fingerprint below |
| Final Linux package integrity | All 571 payload files matched size and SHA256 | Manifest `c5323be275866432dab0ef5f1a5c8f59ff557469606dfe0bcc87932e3f5580d3` |
| Final Linux packaged CLI and HTTP service | CLI returned `[]`, exit 0 and empty stderr; health, editor HTML, JavaScript and workspace/capability checks passed; owned service stopped | Isolated empty state; no proprietary client launched |
| Windows native build on Main before final GUI-subsystem adjustment | 18/18 Core checks and 101 Platform/Creator assertions passed, both self-contained programs published | The two additional Linux assertions test Unix file permissions |
| Real Steam registration on Main | 3,298 files inventoried; 31 critical file hashes recorded | ID `58536515790d496092589f6735bc898e`; writes only to Metallic launcher state |
| Real Campus registration on Main | 3,272 files inventoried; 40 critical file hashes recorded; Campus.exe registered as external launcher | ID `fbcdceaee00e48bfade7e3d125485b98`; existing Campus authentication state was not read |
| Windows GUI-subsystem compilation | Windows and Linux targeted builds passed with zero warnings/errors; Windows apphost PE subsystem 2 | Final native Windows GUI and captured CLI acceptance are recorded separately when completed |
| Existing Python baseline suite after restoration | 40 tests passed | Temporary localhost mock servers; first sandbox-limited attempts failed to create sockets, rerun outside that restriction passed |
| Existing combined Node baseline after restoration and hub removal | 62 tests passed; zero failed, cancelled, skipped or todo | Seven test files covering launcher, creator contracts, support, Discord status, player counts and staff policies |

The final desktop build-input fingerprint is `7d652604d8488e8740891dced3fd0787cc4ef5917e221772ba3353c0b8407608` across 47 files. [Fingerprint definition and exact files](desktop-build-inputs-2026-10-20.json), [fresh Linux receipt](linux-final-package-2026-10-20.json), and [build log](linux-build-2026-10-20.txt) are retained with the source. The local recovery directory did not yet contain Git metadata, so its package correctly records null source-commit fields; the explicit fingerprint identifies its build inputs.

The Windows test-fixture fix creates ordinary NTFS directory junctions in temporary owned fixtures, and removes them before deleting their targets. It did not enable Developer Mode or change device-wide symlink policy. The final launcher uses the Windows GUI subsystem and preserves inherited CLI output handles instead of opening an extra console window for desktop launches.

## Historical browser and Linux desktop acceptance

Before workspace maintenance pruned scratch artifacts, a real Chromium run against the actual local service passed **10/10** checks. It covered the initial 42 result, editing the inputs to produce 100, persistence through save/reload, revisions, JSON round-trips, invalid JSON retention, named port endpoints, 1440/390/320-pixel layouts, readable selectors, and absence of browser errors. The original run was 03:04:41.834–03:04:46.087 UTC.

The [browser evidence record](creator-browser-2026-10-20.json) explicitly identifies its reconstruction from actual earlier tool output. Original screenshots were captured and inspected but subsequently pruned; they are not supplied as if they survived. The browser suite and its exact Playwright 1.56.1 lockfile were restored. No new browser result is inferred from that restoration.

The packaged Linux Avalonia application previously rendered at 1170×810 and its 940×690 minimum size, and exited normally under its 12-second smoke timer. Its original images were also pruned. That check was X11 in the available test environment, not Wayland, a physical VR headset, GPU performance, Android, or high-DPI certification.

## Main-PC records

The task-owned directory is `C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020`.

- `windows-build-pass4.log` contains the earlier native Windows 18/101 acceptance and successful package publication.
- `evidence\steam-import.json` and `evidence\campus-import.json` retain actual successful import results; corresponding stderr files were empty.
- `history-evidence.json` and `rea-context.json` retain the bounded, relevant Codex history evidence.

The actual client roots, versions and hashes are recorded in the [PC inventory](pc-client-inventory-2026-10-20.json). The [REA recovery record](REA_HISTORY_RECOVERY_2026-10-20.md) separates prior reports from independently verified measurements.

## Removed rejected work

The rejected Meadow task-owned PC package, desktop shortcut, Godot user-data directory, Library artifacts and remote feature branch were removed. [PR #72](https://github.com/oncoursemail/Metallic-RC/pull/72) was closed unmerged and titled “Withdrawn: rejected Meadow prototype.” Original installed Rec Room, Campus, Nexa and Meow files were not modified.

The active native world, hub manifest, Godot package/build tests, native workflow, server session supervisor and associated Meadow design/validation documents were removed from the replacement tree. Historical references are identified as withdrawn; they do not define the new build. Source recovery uses the original main and five prior feature/research heads as provenance and does not make the rejected playtest commit a parent of the corrected rework.

## Remaining acceptance gates

No rebuilt Rec Room executable, compatible game login/session protocol, shared two-client world, voice, reconnect, in-game CV3, mobile client, headset acceptance, public hosting or measured game-performance improvement is claimed. Those require their own executable implementation and recorded client/device tests. A launcher compile or local API record is not a substitute.

No signed installer or public release has been issued. Repository policy requires CI to pass before merge; local evidence does not silently waive that gate. Final source publication and native Windows desktop evidence are appended after those actions complete.


## Completed desktop handoff

Final exact-commit Windows packaging, native GUI and creator-service handoff, both real-client verifications, the exact uploaded Linux archive smoke, installed shortcut, archive hashes and the hosted CI blocker are recorded in the [final desktop handoff](FINAL_DESKTOP_HANDOFF_2026-10-20.md). It supersedes the pending final-Windows/publication statements above without relabeling historical evidence.
