# Native multiplayer acceptance tests

These tests exercise the native Godot game. They are separate from `game/bench` and the repository's earlier synthetic tick model. The network harness starts an actual dedicated Godot authority and separate Godot client processes, using the production `Session` autoload and ENet RPCs.

## Run

Use the official Godot **4.5.2** editor binary. Run from the repository root, replacing the executable and evidence paths for your machine:

```sh
godot --headless --path game/native --script res://tests/test_simulation.gd
python game/native/tests/run_network_tests.py --godot /absolute/path/to/godot --output /absolute/path/to/evidence
```

On restricted Linux environments, point `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, and `XDG_CACHE_HOME` at writable task directories. The Python harness sets these for its child processes. It needs permission to create **loopback UDP sockets**. It does not need public network access, another machine, administrator privileges, a GPU, or third-party Python packages.

`--skip-load` runs the activity and recovery scenarios. `--load-only` runs concurrent clients. These flags cannot be combined. Default load is 32 clients, measured for eight seconds after all clients have joined; `--clients` accepts 2–32 and `--load-seconds` accepts 3–30. These short acceptance runs do not establish long-duration service capacity.

## What is checked

The pure Godot suite checks player names and capacity; membership, input sequence and rate limits; finite, normalized inputs; movement and abrupt-turn speed bounds; stale input expiry; barn wall collision and the actual doorway; server-owned jump and floor; reconnect reservations and expiry; shared landmark proximity, cooldown, replay rejection, bell completion, and round reset; progress validation and mutation atomicity; and the compact snapshot codec.

The real network scenarios check:

- Two clients agree on the same three-landmark-and-bell result. The actor walks the actual map through input RPCs; the harness never moves server players directly.
- The actor's datagrams receive a seeded 35 ms delay in each direction, ±8.75 ms jitter, and 2% configured loss. The report records actual datagrams dropped. Reliable ENet traffic still uses the production retransmission path.
- A disconnect and reconnect preserves the same player ID, nearby position, and shared progress during the live server's resume window.
- Before the impaired activity, a separate admitted ENet probe sends a valid baseline followed by replayed, older, oversized, incorrectly typed, identity-spoofing, sequence-leap, and non-finite inputs. It pauses its ordinary input pump and waits for the authority's counter acknowledgment for each case. This avoids assuming delivery of an invalid packet that later valid input could supersede on an unreliable ordered channel. The production RPC validation remains unchanged.
- An incompatible protocol is refused. Every one of 32 load clients sees all 32 connected players, moves through real accepted input, and observes bounded motion. A 33rd client receives an explicit full-room response.
- The harness forcibly terminates the test server twice consecutively after a persisted completion. Each restarted server restores the shared round. The same still-open client reconnects automatically after both crashes using a **new session**, because bearer credentials and player sessions are not persisted. A successful admission resets the fresh-session retry allowance; failed handshake retries stay bounded.
- A corrupt save refuses server startup and remains byte-for-byte intact. Unix save files have mode `0600`. Engine and script errors fail acceptance.

## Wire and authority contract

Simulation runs at 30 fixed steps per second. Snapshots are broadcast at 15 Hz. A full snapshot contains all active and reserved players. Its public API remains a `Dictionary` for the client UI; the network representation is packed and bounded to **1,277 bytes**, including 32 maximum-length player names and a completion result. Position quantization is 2 mm, velocity quantization is 1 mm/s. Derived landmark geometry is loaded from the shared versioned world contract.

Every gameplay request is mapped to the sending peer's admitted membership. Clients submit horizontal direction and a jump request, never position, velocity, landmark state, or completion state. Only the local host can manually reset the round; a dedicated server starts another round 30 seconds after completion. Peer-to-peer RPC relaying and object decoding are disabled.

Reconnect credentials contain 32 cryptographically random bytes. The authority indexes their SHA-256 hashes, rotates the credential on successful resumption, and reserves a disconnected player for 120 simulation seconds. Credentials never enter snapshots, metrics, logs, or saved progress. This provides continuity for a private playtest; it is not account authentication or encrypted Internet transport.

Progress saves contain only `version`, `round_id`, `phase`, `lit`, and `completed_by`. They are enabled only with an explicit state-file path. A complete candidate is validated before restoration. Writes use a private temporary file, flush, and a rename in the same directory. Failed saves retain the dirty revision and retry after five seconds. The previous complete save remains available if writing or replacement fails. The harness establishes process-crash recovery, **not power-loss durability**.

## Reading the evidence

`network-evidence.json` records scenario results, datagram impairments, per-client observations, and server measurements. Per-process JSON and logs provide the supporting record. No test credential is included.

`tick_p50_us`, `tick_p95_us`, and `tick_max_us` measure wall-clock microseconds inside the authority's fixed-step callback, including that step's snapshot construction and send calls. They exclude rendering and parts of ENet polling, and are not CPU utilization or end-to-end network latency. `late_frames` counts fixed-step accumulator overflow. Reports identify the actual operating system and client count; a headless loopback result does not establish Internet, rendered-client, VR, other-device, or public-release readiness.
