# Required movement fields, 28 October 2026

An incomplete owned-protocol pose such as `{"type":"pose","sequence":2,"y":-20,"z":-30}`
previously defaulted the absent `x` to zero. The server accepted and broadcast it,
advanced the room revision and consumed the sequence. This could cause unintended
movement instead of rejecting a malformed update.

All five pose properties now carry `JsonRequired`. This is scoped to the pose
record; unrelated HTTP request serialization, authentication and native response
contracts are unchanged. Explicit zero coordinates remain valid. Missing fields
produce the existing `invalid_json` event before changing room state.

## Verification

The new `Metallic.RC.PoseContract.Tests` executable starts the production backend
on an ephemeral loopback port with disposable storage and synthetic accounts.
It uses independent WebSocket clients, not a model of the room implementation.

- Baseline production code failed the new missing-X check: it emitted a pose
  where an error was expected. A separate initial run also showed missing type
  reached semantic validation instead of required-field validation.
- Fixed code passed 52 transport assertions. Cases omit each of type, sequence,
  X, Y and Z, plus all coordinates. Correcting the message using the same sequence
  proves rejection did not consume it. Exact next events and revision increments
  catch unintended broadcasts; a late observer checks the accepted snapshot.
- Fragmented messages and explicitly supplied zero coordinates remain accepted.
- The unchanged owned-backend suite passed 92 checks over real HTTP/WebSockets,
  including persistence, revocation, reconnect and backup restoration.
- Locked solution build succeeded with zero warnings and zero errors.
- A separate reviewer inspected the production fix and transport regression.

Run with a new evidence path:

```text
dotnet run --project tests/Metallic.RC.PoseContract.Tests -c Release -p:RestoreLockedMode=true -- artifacts/pose-contract.json
```

The existing Windows/Ubuntu backend workflow now builds and executes this
regression. Hosted results are separate from the local results above.

## Native client status

The installed Steam build remains 23191908. A fresh REA 6.5.0 static PE resource
inspection succeeded, and the installed executable and GameAssembly hashes match
the recorded baseline. This inspection does not execute the game or establish
native backend compatibility.

The retained latest actual game attempts are from 22 October, not the older
timeout cited in earlier handoffs. Both failed attachment while loading the
instrumentation agent; the process had exited with code 1 before cleanup. No
agent load, resume, native discovery or native login succeeded. The cause remains
undetermined. No new original-client run or attachment was attempted here.

An offline account-fetch selection also remains unresolved: source bytes exist
in the retained carrier, but no matching Ghidra procedure was available. Neither
the failed selection nor the movement tests establish native login or gameplay.

This change is a review candidate. It does not update the installed backend,
publish a release, or implement original-client login, room transport or voice.

## Reference

[Microsoft: required JSON properties](https://learn.microsoft.com/en-us/dotnet/standard/serialization/system-text-json/required-properties)
documents `JsonRequired` and the serializer's handling of missing properties.
