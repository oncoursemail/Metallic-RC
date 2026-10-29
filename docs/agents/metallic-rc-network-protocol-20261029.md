# MGM2 network batch preparation

Board #364. Based on PR #102 commit
`820e23f449ab18184b23e4441f9c4fe73e29ccf1`.

`network_protocol.rs` is a standalone, pure module. It is deliberately not
exported from the crate or connected to a transport, process launcher, WFP
adapter, worker role, or original-client profile. Phase 1 execution remains
unchanged. This checkpoint does not make phase 2 ready to execute.

## Closed contract

MGM2 has exactly 128 bytes: magic/version/kind, phase and probe index, a
16-byte session nonce, a little-endian sequence number, retained PID and
creation time, destination port, raw signed exit-code bits, optional exact
32-byte MRU1 UDP metadata, and zero reserved bytes. Unknown tags, lengths,
nonzero reserved bytes, zero identities and unbound metadata are rejected.
MRU1 metadata is producer-reported data, not operating-system attestation.

The planner requires four positive controls in fixed TCP4, TCP6, UDP4, UDP6
order, then exact guard readback supplied by a trusted host adapter. Each
planner starts without a helper identity. Only Ready, after all four
before-controls, binds the actual verified helper once; no dummy identity,
early elevation requirement or replay of earlier controls is needed. Each
blocked probe requires Register, Ack, Result, ProbeDone and Next, in order.
Register retains the identity before Ack can return a one-time ResumeOnce
action. Nonce, sequence, probe index and root identity bind each transition.
The same guard scope remains throughout all four blocked probes.

Raw exit 20 is an accepted denial outcome only when it agrees with the
retained process's actual exit status. A canonical UDP timeout (75596) needs
a Receive-stage MRU1 record with four bytes sent and an exact fresh outbound
classify-drop observation. The observation must cover App-ID, UDP/family,
loopback endpoints, both oriented ports, required header presence, the
current CONNECT filter ID and resolved layer ID, direction 0x3901, and the
high-side Ack-to-exit UTC window. Missing, partial or failed evidence refuses
the result. Inbound-only evidence and timeout alone never qualify.

Guard arrays use CONNECT4, CONNECT6, RECEIVE4, RECEIVE6 order. No addresses,
paths, blobs, SIDs or packet contents are stored or emitted by this module.

Release becomes available only after all four outcomes and job/drain
cleanup confirmations. Four after-controls are permitted only after the
same filter IDs are absent, sublayer/session cleanup is confirmed, and the
expected helper has exited successfully. All four after-controls are needed
for Complete. Reused identities, skipped frames and replayed Ack fail closed.

Every incomplete/failed batch retains all registered blocked-root stamps,
including roots from earlier completed probes. Recovery requires the
launcher and every retained root to have exited; a disconnected launcher
does not make a later, possibly unregistered child safe to ignore.

## Verification

Run from the repository root with an installed Rust compiler:

```powershell
rustc --edition=2021 --test tools/client-lab/guarded-medium-control/src/network_protocol.rs -o "$env:TEMP/network-protocol-tests.exe"
& "$env:TEMP/network-protocol-tests.exe"
```

The authored module's 17 pure tests pass. They exercise the full 31-frame
batch, codec boundaries and crash-bit preservation, nonce/order/replay
refusal, exact retained exits, drop-field mutations and presence bits,
metadata binding, late helper binding, guard/removal failures, root recovery,
and clock bounds.
No socket, process, WFP, UAC, worker or original-client action ran.

The parent is adding a separate direct-rustc Windows/Linux CI workflow.
Hosted results are not established by this local note.

## Limits for future integration

Host values are typed assertions made by a future independently reviewed
adapter, not proof created by this pure planner. The adapter must retain
and verify real handles, pin identities, authenticate its peer, check the
worker-specific policy and exact readback, collect fresh bounded evidence,
and enforce real job/drain/helper lifetimes. It must never supply wire JSON
as Host authority or hold the target's kill-on-close job in the high helper.

The planner checks monotonic 30-second startup and 90-second batch limits
only when called. It cannot interrupt blocked I/O or enforce the external
130-second hard watchdog. UTC values use Windows FILETIME ticks and the
kernel observation window is at most ten seconds. Clock and header-encoding
uncertainty must remain inconclusive in the future adapter.

This module makes no claim about delegated DNS/services, complete isolation,
arbitrary targets, original-client readiness, or any new runtime permission.
