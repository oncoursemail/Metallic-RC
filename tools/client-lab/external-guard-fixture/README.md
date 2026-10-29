# External guard: authored fixture only

This crate prepares an optional WFP diagnostic. It cannot launch Rec Room or
select another executable. The default and `--plan` roles only print a closed
plan with `executionReady:false`, `approvalGranted:false` and
`filtersInstalled:false`. No WFP session is opened in those roles.

Native execution must remain within the owner's current authorization and the
reviewed test scope, with normal Windows consent retained. Preparation checks
do not establish native success; attempted-run evidence is recorded separately.
The program never
self-elevates, changes ACLs, starts services, adds persistent rules, or modifies
an original client. A permitted future run needs normal Windows approval through
the separately reviewed controller; permission failures remain failures.

## Native fixture contract

`--fixture` reserves only `fixture-report.json` beside its own executable, using
CREATE_NEW before WFP access. An existing result refuses the run. Stage a reviewed
binary in a new private one-run directory. A pending/partial result or nonzero
exit is not success; no arbitrary output path is accepted. Final JSON is bounded
to 2048 bytes and flushed. This is trusted local output, not attacker-proof
attestation.

The fixture refuses another instance through a fixed global named mutex. App-ID
filtering is executable-path scope, not PID scope: it can affect any process
using the same executable path, including the controller and its probe children.

1. Four fresh same-executable children must exchange a fixed four-byte marker
   with owned loopback listeners over TCP/UDP, IPv4/IPv6. IPv6 unavailability or
   any failed positive baseline refuses the control; it is never a skipped pass.
2. One dynamic session and transaction create one unique sublayer and exactly
   four BLOCK filters: ALE AUTH_CONNECT and AUTH_RECV_ACCEPT, V4 and V6. The sole
   condition is equality with the current executable's API-derived App-ID blob.
   Every ID, unique key, sublayer, layer, blob, action, flag and condition is read
   back before blocked probes may start. There are no destination exemptions.
3. Four fresh children must report explicit Winsock `WSAEACCES` (10013), encoded
   as exit 20. Only that error counts as a blocked observation. Other positive
   raw errors in 1..65535 use a canonical `0x10000 | error` diagnostic exit and
   fail the phase. A missing/bad reply or unknown error uses exit 21. Handshake
   failures use 1. Crashes never pass. Tagged 10013 is noncanonical and rejected.
4. The controller stops and confirms all tracked children and the owned Windows
   job empty before deleting filters. All four exact runtime IDs must be absent,
   the sublayer deleted, and the dynamic session closed successfully.
5. Four fresh positive probes must work again. They are stopped/reaped, and the
   job must be empty before a success result can be written.

The only spawn target is `current_exe()`. Children wait for a pipe handshake until
assigned to a KILL_ON_JOB_CLOSE job. Child stderr is null; the bounded UDP
diagnostic pipe is described below. The internal `--probe tcp4|tcp6|udp4|udp6 PORT` role accepts
only loopback and the fixed marker; it has no WFP or command/target-path API.
TCP child stdout and stderr are null. UDP stdout carries only a fixed 32-byte
diagnostic frame, explicitly flushed; the parent inspects it only after a known
retained process exit and an exact available-byte count. It never waits for EOF.
Short, extra or invalid metadata is unavailable. A producer write/flush failure
adds a generic failure rather than reporting successful diagnostic transport.
That internal role can be manually invoked against a loopback port if its stdin
handshake is supplied; it is not an authentication boundary.

The monotonic 90-second budget latches failure, is checked around admission API
calls and every probe/spawn/handshake, and cannot accept late success. A watchdog
stops the owned job at 90 seconds; the native role has an independent 130-second
failure exit covering setup. A stalled native call does not rely on that call's
cooperation. Pending children retain the guard owner until they terminate or the
hard failure boundary is reached. Abrupt termination leaves cleanup unverified.

## Limits

This is not a complete network sandbox or proof of zero account contact. It
tests owned loopback sockets only. Filter readback is configuration evidence,
not proof of every Windows network path or arbitration outcome. No DNS, proxy,
service/IPC delegation, unrelated helper executable or whole-machine isolation is
established. No service or existing policy is blocked or disabled.

Dynamic objects disappear on session close or owner death. Normal cleanup orders
child termination before filter deletion, but ordering between automatic filter
removal and job cleanup on owner/process/OS failure is unproven. Do not reuse this
fixture as authorization to remove existing guards or run an original client.

## Preparation checks

Dependencies are exactly cached `windows-sys 0.61.2` and `windows-link 0.2.1`,
with registry checksums in Cargo.lock. No packages were installed or downloaded.
The existing MSVC Rust toolchain compiled the Windows API adapter successfully.

```text
cargo build --offline --locked
cargo test --offline --locked --lib
target/debug/metallic-external-guard-fixture.exe --plan
cargo test --offline --locked --bin metallic-external-guard-fixture accepted_stream_
cargo test --offline --locked --bin metallic-external-guard-fixture udp_metadata_pipe_
```

Thirty-three pure fake-API tests cover four-filter order, partial admission failures,
readback field mismatch, pending-child refusal, deletion/read/close failures,
closed roles, monotonic deadline latching, timeout setup admission, strict
Winsock classification and crash/unknown exit refusal. Readback failures now name
the first mismatched field using a static category. Every field remains checked,
including a final exact equality check; no flags are masked or tolerated.
A failure may include bounded unsigned
`requestedFlags` and `observedFlags` diagnostics from the last actual readback.
The fixture now explicitly requests exactly `FWPM_FILTER_FLAG_INDEXED` (`0x40`),
with a compile-time SDK equality check. The input, expected rule and admission
policy share that exact value. Zero, disabled flags and every additional bit are
rejected; there is no mask or fallback. No path or App-ID blob is emitted,
and the successful nine-field result schema is unchanged. These tests do
not call WFP or create sockets/children. Only the pure plan was executed.
Failure-only probe metadata names the closed before/blocked/after phase and
TCP/UDP IPv4/IPv6 protocol, plus the retained child exit or null. Canonical tagged
errors may be decoded as `childReportedSocketError`; this is the authored child's
diagnostic namespace, not independent OS attestation. The existing child-exit path still requires exact exit 0 or 20 for the
corresponding phase. The separately verified blocked-UDP path below preserves
the raw timeout code and requires current outbound kernel evidence. All 65,535 raw-error tags
are checked for canonical decoding and refusal by the pass predicate.
The separate Windows binary tests use only authored loopback TCP sockets, with
no WFP or native fixture role. They exercise the actual accepted-stream helper:
explicit blocking mode precedes finite read/write timeouts, and a delayed marker
must be awaited. The nonblocking listener is unchanged. Timeout bounds are
checked separately. This guards against inherited nonblocking mode causing an
early `WouldBlock` and connection drop. See [Winsock accept inheritance](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept).
Three additional Windows binary tests use owned anonymous pipes to check exact
32-byte records after writer close, short/extra data, failed reads and destination
mismatches. They do not enter a fixture/probe role or call WFP.

## Read-only UDP drop diagnostics

A UDP timeout alone remains failure. For eligible blocked UDP receive timeouts,
diagnostics read existing event-collection state and inspect only exact scoped
`Enum2` drop events before deciding that probe. Collection must already be enabled; this program never calls
`SetOption`, changes auditing/providers or enables global collection.

Queries require this executable's exact App-ID, UDP, loopback family/addresses,
both owned ports, event type and probe time window. Returned events must also
match this run's filter ID, resolved layer ID, required header fields and exact
direction. Child-outbound and server-inbound counts stay separate. Metadata is
producer-reported, with no PID attestation. No App-ID/path, SID, payload or other
application data is emitted. One header encoding is used; mismatches remain
inconclusive rather than trying another byte order.

Enumeration inspects at most two pages of eight events for each direction, with
a shared 64 KiB event-data budget and two-second cooperative deadline. Synchronous
Windows RPC calls are not cancelled by that deadline; the existing 90/130-second
watchdogs remain. Arrays are freed and enumeration handles are destroyed on exit.
Missing, disabled, unsupported or incomplete observations remain unavailable or
inconclusive. A timeout can satisfy only the blocked UDP phase when all of these hold:
canonical child-reported 10060 (raw exit 75596); matching IPv4/IPv6 metadata;
nonzero source and destination ports; exact receive operation after sending
four bytes; valid bounded probe times; and completed matched diagnostics with
at least one outbound drop and no API failure. The native reader must match
this run's current verified filter, layer, App-ID and complete tuple. Inbound-only
matches, timeouts alone, disabled collection, unavailable/capped/deadline results
and API errors remain failures. No raw child code is rewritten to 20.

The existing successful nine-field schema remains unchanged. Success still
requires all four before positives, all four blocked probes (including UDP6),
empty owned children/job before verified filter removal, and all four after
positives. The resolver queries current evidence for each eligible blocked UDP
probe; a successful UDP4 confirmation does not skip UDP6 or cleanup. Before/after
positives and TCP never use kernel evidence as an alternative to their expected
exit. No original-client readiness or full sandbox claim follows from this test.
API failures include only a closed collection/layer/enumeration stage and the
actual returned unsigned error code. These fields contain no exception text.
Rustfmt is unavailable in the existing toolchain; no component was installed.

Microsoft references: [dynamic session lifetime](https://learn.microsoft.com/en-us/windows/win32/fwp/object-management),
[app-ID filters](https://learn.microsoft.com/en-us/windows/win32/fwp/permitting-and-blocking-applications-and-users),
[ALE layers](https://learn.microsoft.com/en-us/windows/win32/fwp/ale-layers),
[filter readback](https://learn.microsoft.com/en-us/windows/win32/api/fwpmu/nf-fwpmu-fwpmfiltergetbyid0),
[arbitration](https://learn.microsoft.com/en-us/windows/win32/fwp/filter-arbitration).

The source change was prompted by an actual authored-fixture observation: exact
flags 64/64, blocked UDP4 raw exit 75596 / receive timeout 10060, send bytes 4,
and a matched current-filter outbound kernel drop (one outbound, zero inbound).
That earlier revision correctly remained failure under its strict exit-only
policy; it is evidence for the added narrow predicate, not a passing four-probe
run. The new source has not been run with WFP here.

Local validation: locked offline release build and 39 pure/mock library tests
passed, retaining every existing assertion and adding six evidence/lifecycle
tests. They exercise both families, every required metadata/stat field, every
bounded raw error tag, fresh re-query rather than stale counts, and the shared
phase path through UDP6, fake cleanup and all after positives. Missing outbound
evidence or failed cleanup cannot reach after positives. The two existing owned
TCP controls and three anonymous-pipe controls also passed without WFP or
privileged process roles.
