# Metallic-RC integration baseline

**Recorded:** 2026-10-20 UTC. **Scope:** local source integration for the owner-requested Metallic-RC completion work. This record does not mark a game release or a hosted CI success.

## Exact upstream provenance

Repository: [oncoursemail/Metallic-RC](https://github.com/oncoursemail/Metallic-RC), private repository ID `1420186760`.

| Source | Exact commit | Integrated content |
| --- | --- | --- |
| Main | `20abe16e8137874615c4cd04fe8c3d4986d0ed6c` | Existing website and crow artwork, Atlas control lab, player-count service, Discord skeleton, staff policy, progress and test infrastructure |
| [PR #71](https://github.com/oncoursemail/Metallic-RC/pull/71) | `87ee10f400f47df7991600a3208c97cd75faacee` | Canonical Metallic-RC names, current repository links, shared project identity map, Go module/import names |
| [PR #70](https://github.com/oncoursemail/Metallic-RC/pull/70) | `121c634092a24a9409d2c8b19fcbe525c33b75ca` | Independent launcher choices, preference store, installation/removal planner and workflow |
| [PR #56](https://github.com/oncoursemail/Metallic-RC/pull/56) | `1d08514f0509364737eade2700180e175edd6632` | Meadow manifest, creator/Roomie/watch/workshop contracts, support policy, proposed runtime ADR and tests |
| [PR #65](https://github.com/oncoursemail/Metallic-RC/pull/65) | `d60a6cb67d1027e422c34c2d6c6cf1882f7c5276` | Capacity model, reference input admission gate, tests, workflow and global infrastructure research |
| [PR #55](https://github.com/oncoursemail/Metallic-RC/pull/55) | `f2286a3c02a8cc0a222ec2635782dad9a088f7e0` | Original world, Academy, creator exchange and companion research |

The six exact commit objects, their complete trees, and 150 unique file blobs were read through the connected GitHub API. All 238 reconstructed Git objects matched their upstream Git SHA-1 identifiers before checkout, including the signed main commit and binary crow asset. This is an exact shallow source checkout, not a synthetic re-creation of the main commit. Unavailable older ancestors remain explicit shallow boundaries in the local Git metadata.

Integration order was main, #71, #70, #56, #65, then #55. The current README retains all Atlas and original project links and adds the independent PR contributions. Current repository links in the added documents use Metallic-RC. The shared launcher still uses the Metallic family brand, while its original social-game component displays Metallic-RC; stable component IDs and stored preference keys are preserved.

The `agent/*` branches were inspected during the audit but were not integrated. No generated Python caches or dependency directories are part of this baseline.

## Local checks

Environment: Python 3.12.14, Node 24.19.0, Linux. The source packages retain their declared Node 22 minimum; this run does not establish a Node 22 compatibility result.

| Command | Result |
| --- | --- |
| `python -m unittest discover -s tests -p 'test_*.py'` | 40 tests passed |
| `node --test services/launcher/tests/*.test.mjs game/tests/*.test.mjs services/support/tests/*.test.mjs services/discord-bot/tests/*.test.mjs services/player-count/tests/*.test.mjs services/staff/tests/*.test.mjs` | 64 tests passed |
| `git diff --check` | Passed |

Initial restricted-sandbox attempts could not create loopback sockets. The same existing test suites passed after approved execution outside that socket restriction. The blocked Node run was stopped. No test assertions were removed or bypassed.

Go is not installed in this local environment, so the Atlas Go/race/build suite was not rerun for this integration. The launcher UI retains its existing implementation with naming text changes; no new browser visual acceptance is asserted in this record. Hosted checks must still be run at the final proposed commit; prior exact-head PR checks include an account billing/spending-limit startup blocker recorded in PR #71.

**Subsequent validation in the same work session:** the official Go 1.27.2 archive was downloaded and its published SHA-256 verified. The Atlas `go test -race ./...`, `go build ./...`, `go vet ./...`, and gofmt check then passed. This follow-up supersedes the Go-tooling limitation in the original baseline run; it does not change the hosted-CI requirement. The later Meadow playtest was rejected by the owner and withdrawn. Its source, package links and validation document have been removed. The [compiled-client rework](COMPILED_CLIENT_REWORK.md) records the replacement direction.

## Implementation boundary

This baseline brings the existing work together. It does not supply an engine client, a real authoritative room simulation, a signed native installer, spatial voice, device certification, public hosting, or a production session runner. Those require separately recorded implementation and acceptance evidence. The original two-player room remains the next playable milestone.

No remote branch was changed, no PR was merged, no issue was closed, and no live service was deployed by this local integration step. Subsequent completion work may build on this baseline and must report its own evidence.
