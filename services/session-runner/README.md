# Metallic-RC private session runner

This Python standard-library CLI supervises one native Metallic-RC session on **Linux**. It validates the reviewed build manifest and both game files, starts a fixed loopback command, waits for matching readiness, and restarts a crashed process with bounded backoff. Shared Meadow round progress survives restarts through the native server's state file.

This is an implemented private-lab foundation for [issue #67](https://github.com/oncoursemail/Metallic-RC/issues/67). It does not complete the signed production runner, public join-ticket, isolation, fleet-fencing, or deployment requirements in that issue.

## Run a reviewed native build

Use Python 3.11 or newer as an **unprivileged account**, with a build produced by [`scripts/build_native.py`](../../scripts/build_native.py). The `server` and `linux` targets have known executable names; Windows packages are rejected. Every path passed below must be absolute, with no symlink components or parent traversal.

```bash
python3 services/session-runner/session_runner.py run \
  --manifest /absolute/reviewed-build/server/BUILD.json \
  --data-dir /absolute/private-sessions/meadow-1 \
  --port 27841
```

The data directory may be created as a new leaf under an existing parent. If it already exists, it must belong to the running account and have mode `0700`. Existing state, receipts, and the lock must be private files owned by that account. Build files may belong to that account or root, must not be writable by group or others, and must not be symlinks or hardlinks.

For a separately reviewed manifest digest, add `--expected-manifest-sha256` with its exact lowercase SHA-256 value. Regardless of whether this pin is supplied, the runner pins the manifest it first verified for its entire lifecycle; a later manifest or package change stops recovery instead of silently adopting a new build.

The launcher command is constructed internally:

```text
Metallic-RC-Server.x86_64 --headless -- --server --bind 127.0.0.1 --port 27841 --state-file /absolute/private-sessions/meadow-1/meadow.json
```

Only the known build executable changes for the `linux` target. There is no executable-path, shell-command, environment, public-bind, or HTTP request option. The operator chooses one unprivileged port in `1024..65535`; it remains fixed for every attempt. The runner cannot open a public listener through configuration.

Stop the foreground supervisor with **Ctrl+C** or SIGTERM. It sends SIGTERM only to its currently owned child, waits for the configured grace period, and uses SIGKILL on that child only if necessary. Receipt PIDs are never used to adopt, signal, or kill another process.

## Integrity and readiness

`BUILD.json` must use the exact `metallic-rc-playtest-build-v1` build schema and known filenames. The executable **and the adjacent `.pck` are required**. All declared files are checked for byte length and SHA-256; mismatches or missing files refuse execution. Required assets are copied into a private directory under `verified-builds/<manifest-sha256>/`, checked during copying, and checked again before reuse. This keeps an attempt on its verified package even if the original build folder is later replaced.

The build manifest currently declares `signed: false`. These checks establish **file integrity against a reviewed local manifest**, not publisher authenticity. A `signed: true` field is rejected because this version does not implement a signing protocol. Only use an operator-reviewed build; creating a new manifest around an unknown executable does not make that executable trustworthy.

Readiness requires one complete stdout line beginning with `METALLIC_READY ` and exactly these fields:

```json
{"protocol": 1, "port": 27841, "bind": "127.0.0.1", "server": true}
```

The port must equal the configured port. Different binds, extra/missing fields, duplicate JSON keys, Boolean numbers, nonfinite numbers, or repeated readiness lines fail. Arbitrary child output is discarded. Output is limited to 4 KiB per line and 1 MiB per attempt; raw output and inherited credentials are not written into receipts. A readiness line is the reviewed server's startup report; actual protocol/client acceptance remains a separate test.

## Recovery and stored state

| Option | Default | Bounds and behavior |
| --- | --- | --- |
| `--max-attempts` | `3` | 1–10 total starts, including the first |
| `--ready-timeout` | `10` seconds | 0.05–60 seconds to report readiness |
| `--stop-timeout` | `3` seconds | 0.05–10 seconds each for terminate and kill waits |
| `--backoff` | `0.5` seconds | Initial delay, 0.01–10 seconds |
| `--backoff-cap` | `8` seconds | Maximum delay, at least initial delay and at most 30 seconds |

Unexpected process exit, including exit code zero, or a readiness timeout triggers the bounded restart policy. Delay doubles after each failed attempt up to its cap. Invalid readiness, changed build bytes, invalid state, or unsupported ownership fail immediately. Restarting the supervisor manually begins a new run with a new attempt budget; this CLI installs no recurring service.

The data directory is protected by an exclusive, nonblocking `flock` for the supervisor's lifetime. A second supervisor cannot use the same directory. A stale lock file does not block a new owner after the old process exits. Use a local filesystem with working advisory locks; cross-machine shared storage and distributed fencing are outside this runner's scope.

Files managed under the session directory are:

- **`meadow.json`** — native state version 1: round ID, collecting/complete phase, unique `west`/`east`/`grove` landmark IDs, and the supported completion name. The entire state is validated before startup and restart; corrupt, unknown, unreadable, oversized, or improperly owned state is preserved and refused.
- **`receipt.json`** — an atomically replaced private lifecycle receipt. It records the run ID, build digest/commit, attempt, configured bind/port, current child PID, readiness, reason code and final state. It contains no authentication tokens, environment values, or raw child output.
- **`.runner.lock`** — lifetime advisory lock; its presence alone does not mean a process is running.
- **`verified-builds/`** — checksum-verified private executable/pack copies. The runner does not delete earlier builds or saves.
- **`runtime/`** — per-session XDG data/config/cache directories for the child. A minimal child environment avoids inheriting account credentials or dynamic-loader overrides.

Native saves use a same-directory temporary file and rename and are written at supported gameplay transitions. After a server crash the shared round returns; players join again. Cross-process identity/resume-token continuity is not claimed.

Lifecycle transitions are `starting → ready`, followed by `backoff → starting` on a recoverable failure, `failed` when the attempt budget or a validation gate is hit, or `stopped` after an operator stop. Exit code `0` means requested stop, `1` means a recorded lifecycle failure, and `2` means a rejected configuration or filesystem/process setup error.

## Checks and remaining gates

```bash
python3 -m unittest discover -s services/session-runner/tests -v
```

The suite uses real finite fixture subprocesses with reviewed test manifests and no network listener. It covers executable/pack integrity, manifest pinning, crash recovery with preserved progress, bounded backoff, readiness mismatch/timeout, corrupted saves, exclusive directory locking, symlink/hardlink/ownership rejection, output limits, SIGTERM, and killing only the owned child. Root test execution explicitly supplies `--allow-root-for-tests`; that override is recorded and is not a production setting.

Actual Godot validation must separately run the exported package as the intended unprivileged account, reach native readiness, connect protocol clients, mutate the round, kill the owned child, observe recovery, reconnect and verify the restored round. Stop the supervisor and confirm the owned server exits and its loopback port is released. No real-Godot result is claimed by the fixture suite.

There is no cgroup/CPU/RAM quota, seccomp/AppArmor policy, container isolation, signed publisher trust, player-ticket service, Atlas reservation/fencing integration, multi-host control, public edge, or system-service installation here. SIGKILL or an abrupt supervisor crash cannot run its cleanup handler; this version does not claim parent-death cleanup or adopt a child using a stale PID. A surviving child must be handled by an operator or a separately designed service boundary. Keep the full issue #67 open until its production requirements and real fleet acceptance pass.
