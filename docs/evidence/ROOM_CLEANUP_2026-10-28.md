# Room membership cleanup after storage faults

28 October 2026. Candidate on `codex/rc-room-cleanup-20261028`, based on
`f09769f`. No installed service or native-client profile was changed.

The owned room monitor checks its session against SQLite every second. A storage
exception aborted its socket, but `Connect` then awaited the failed monitor
inside its cleanup block. Unexpected exceptions such as `SqliteException` escaped
that await before membership removal, connection-count decrement, empty-room
removal and peer disposal. The account could remain falsely connected and keep a
capacity slot until the host restarted.

Nested `finally` blocks now release membership, remove an empty live room and
dispose the peer even when awaiting a failed sender or monitor throws. The
existing exception filter is unchanged: unexpected storage failures still
propagate to the host's error handling instead of becoming successful operations.

## Reproduction and verification

The new `Metallic.RC.RoomCleanup.Tests` executable uses the production backend,
an ephemeral loopback port, a newly generated temporary database, a synthetic
account and independent real HTTP/WebSocket connections. It does not connect to
the installed backend.

Each fault cycle renames only that disposable database's `sessions` table while
a socket is idle. No client message is sent after the rename, so the production
session monitor encounters the missing table. The test observes the failed
connection and waits for zero registered connections before restoring the table.
It then verifies that the original session remains usable and the same account
can reconnect to its capacity-one room. Changed epochs establish that empty live
rooms are removed. Three successive fault cycles catch accumulated leaks; an
ordinary disconnect is also checked.

- The unchanged baseline failed at the first membership-release assertion after
  the monitor aborted its socket. Its private console log is retained in the
  ignored local artifacts directory; no passing baseline result was written.
- The fixed implementation passed **29 real-transport assertions** over three
  monitor-storage faults and a normal disconnect.
- The locked solution build completed with **zero warnings and zero errors**.
- The unchanged owned-backend integration suite passed **92 checks**, including
  revocation, reconnect, persistence and backup restoration. The separate pose
  contract regression passed **52 checks**.

The CI candidate adds this test's paths and locked build/execution to the existing
Windows/Ubuntu backend workflow. Existing checks, permissions and matrix remain.
Hosted results and independent review are separate gates.

Run with a new output path whose parent already exists:

```text
dotnet run --project tests/Metallic.RC.RoomCleanup.Tests -c Release -p:RestoreLockedMode=true -- artifacts/room-cleanup.json
```

This regression proves owned-protocol cleanup under the specified injected
storage failure. It does not prove arbitrary database repair, original Rec Room
login, gameplay, voice or a production deployment.
