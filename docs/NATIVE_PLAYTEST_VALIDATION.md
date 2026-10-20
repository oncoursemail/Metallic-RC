# Native Metallic Meadow — private playtest validation

Date: **2026-10-20 UTC**. Status: **tested development candidate, unsigned, not a public release**.

This checkpoint adds an actual native desktop gameplay loop to Metallic-RC. It integrates the exact existing naming, launcher, Meadow-contract, infrastructure, and research PR sources recorded in [the integration baseline](INTEGRATION_BASELINE_2026-10-20.md). The [native guide](../game/native/README.md) describes how to play and build it. The [experiment ADR](ADR_NATIVE_PLAYTEST.md) limits the Godot decision to this experiment.

The distributed packages' `BUILD.json` and the accompanying evidence bundle identify the final source commit, staged-source digest, executable/pack hashes, target, and unsigned status. This document summarizes the checks; it does not replace those exact artifact receipts or hosted CI.

## Working gameplay

- Host a private Meadow or join a known address with a native desktop client.
- Walk, turn, jump, meet other animated visitors, and navigate the barn doorway and authored collision bounds.
- Discover the canopy, orchard, and little-green markers together, then ring the barn bell. The authority owns movement, proximity checks, progression, and round reset.
- Inspect actual player positions, connected counts, and discoveries in the Meadow watch. Open settings, change saved bell volume and mouse sensitivity, use reduced decorative motion, leave, and reconnect.
- Keep shared activity through a server process restart when an explicit state file is configured. Brief disconnections can resume the same live session; process restarts create fresh admitted identities.

The golf lane and treehouse are scenery. This implementation does not include a golf game, climbing, VR, account login, spatial voice, friends, matchmaking, a creator editor, marketplace, or public hosting.

## Executed checks

| Check | Recorded result |
| --- | --- |
| Godot 4.5.2 simulation and snapshot suite | **187 checks passed** on Linux and Windows. Includes movement limits, sharp turns, stale/replayed/malformed input, room admission, collisions, jump/landing, shared activity, resumption, strict saved state, and bounded snapshot encoding. |
| Real input-guard probe | One valid baseline and **seven invalid input cases** were individually acknowledged by the real authority. The rejected cases could not move the probe player. |
| Two-player shared activity | Two separate native ENet clients completed all three markers and the bell on Linux and Windows with real datagram delay, jitter, and loss. Both saw the shared result. |
| Live reconnect | The activity client rejoined the same running authority with its identity and shared progress retained. |
| Repeated process-crash recovery | The same client recovered after **two consecutive forced server crashes** on Linux and Windows, received fresh admissions, and retained the saved shared round after each restart. |
| Protocol and state refusal | Wrong protocol denied; malformed saved state prevented readiness and was preserved for inspection. |
| 32-player admission and movement | **32 separate Godot ENet client processes** admitted; every client observed 32 visitors and bounded movement. A 33rd client was explicitly refused. |
| Native UI input and visual inspection | Actual mouse click on Host meadow, keyboard movement of **3.88 metres**, Tab watch, and Esc overlays succeeded. Title, gameplay, watch, and pause captures were inspected at 1366×768. |
| Session runner | **23 Python tests passed**, including real fixture subprocess crashes/restarts, digest tampering, private ownership, locks, readiness, timeout/output limits, graceful stop, and owned-child termination. The accompanying bundle records the separate real-export runner proof. |
| Export provenance | **Nine tests passed** for committed-source staging, dirty snapshot labeling, source/template/notice integrity, path/link refusal, no success-looking output after a failed export, preserved earlier receipts, and portable ZIP metadata. |
| Existing repository regression | **40 existing Python tests and 64 Node tests passed** after source integration. The nine added build tests are additional to that baseline. |
| Go Atlas regression | `go test -race ./...`, `go build ./...`, `go vet ./...`, and the gofmt check passed with Go 1.27.2. Atlas remains a separate control-plane lab. |

The final Linux activity/recovery scenario completed in **42.438 seconds**. The equivalent Windows 11 scenario completed in **51.311 seconds**. Each used the same runtime and test implementation. Successful network scenarios also scanned their process logs for engine and script errors. These are finite development scenarios, not continuous public-service certification.

### Network impairment and capacity measurements

The activity relay operated on real UDP datagrams with **35 ms configured one-way delay, ±8.75 ms jitter, and 2% configured loss**. The final Linux activity run dropped 38 of 1,412 observed datagrams; the random realized loss need not equal its configured probability. Input guard probes run separately with acknowledged delivery so the test does not confuse an unreliable packet being dropped with an authority accepting an invalid command.

The final capacity run used a Linux x86-64 KVM environment exposing nine logical CPUs and an AMD EPYC 9V74 model. It held the measurement window for **eight seconds after all 32 clients were admitted**, with 306 recorded authority callbacks across the server run:

| Authority callback measurement | Value |
| --- | ---: |
| Median | 0.537 ms |
| 95th percentile | **1.450 ms** |
| Maximum | 3.149 ms |
| Accepted inputs | 7,121 |
| Rejected inputs during capacity scenario | 0 |
| Recorded late simulation frames | 0 |

These timings measure the authority callback's wall-clock duration. They are not total CPU usage, end-to-end player latency, rendered frame rate, Internet hosting capacity, or a comparison with another engine. The harness verifies the connected count at the beginning and end and checks every client's observations; it does not establish uninterrupted connectivity at every instant of a long soak. Headless clients do not measure GPU or VR performance.

## Native implementation boundaries

The authority simulates at 30 Hz and sends snapshots at 15 Hz. Clients submit sequenced movement intent and proximity-based interactions. They cannot submit a trusted world position. The snapshot codec is bounded to **1,277 payload bytes** at 32 players with maximum-length names; its position quantization is at most two millimetres. This bound excludes transport framing and the reliable admission message.

Player display names are 2–20 restricted ASCII characters. Reconnect credentials use cryptographically random bytes; server indexes use their hashes. Tokens are excluded from snapshots, metrics, test reports, and activity saves. The transport is intended for trusted private testing and is not an encrypted, authenticated public account system.

Saved activity is validated before authority readiness. The server uses a same-directory temporary file and replacement, preserving invalid input for inspection. The test proves process-crash recovery; power-loss durability and a historical backup system are not claimed. The private session runner verifies local hashes and supervises one owned child. A checksum is not a publisher signature, and a restricted process environment is not a complete security sandbox.

## Reproduce the checks

Use the official Godot 4.5.2 stable editor and Python 3.11 or newer. Substitute an absolute editor path for `godot` when it is not on PATH.

```sh
godot --headless --editor --path game/native --import --quit
godot --headless --path game/native --script res://tests/test_simulation.gd
python3 game/native/tests/run_network_tests.py --godot /absolute/path/to/godot --output /absolute/path/to/fresh-evidence --clients 32 --load-seconds 8
python3 -m unittest discover -s services/session-runner/tests -v
python3 -m unittest discover -s tests -p 'test_native_build.py' -v
```

For the equivalent Windows activity/recovery scenario, add `--skip-load --clients 2`. The test README explains [receipt fields and scope](../game/native/tests/README.md). Build all three target packages with [the native build helper](../scripts/build_native.py) and then verify the exported binaries on their target platforms. Export success alone is not runtime acceptance.

## Review and release gates

[The development rules](DEVELOPMENT.md) require passing CI before merge and prohibit closing features merely because code exists. The new native workflow includes Linux and Windows scenarios plus runner and build-provenance tests. Hosted results must be checked at the proposed head.

The prior exact-head GitHub runs were blocked **before test steps** by the account billing/spending-limit startup condition recorded in [PR #71](https://github.com/oncoursemail/Metallic-RC/pull/71). Local passing tests do not satisfy that hosted gate. This work is proposed on an isolated draft PR; it does not merge main or close the broader roadmap issues.

Before the full first-playable milestone can close, Metallic-RC still needs signed distribution, independently hosted admission, the account/ticket boundary, voice and VR/device acceptance, and the relevant operational and moderation gates. Creator tools and the economy remain later roadmap work. No production fleet, firewall, public endpoint, or external community integration is established by this checkpoint.
