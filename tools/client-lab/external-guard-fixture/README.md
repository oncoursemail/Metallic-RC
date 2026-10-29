# External guard: authored fixture only

This crate prepares an optional WFP diagnostic. It cannot launch Rec Room or
select another executable. The default and `--plan` roles only print a closed
plan with `executionReady:false`, `approvalGranted:false` and
`filtersInstalled:false`. No WFP session is opened in those roles.

**Native execution is pending separate owner approval and review.** No native
fixture or game run was performed while preparing this crate. The program never
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
   as exit 20. Only that error counts as a blocked observation. A missing/bad
   reply, timeout (10060), connection refusal (10061), or unknown error uses exit
   21 and fails the phase. Handshake failures use 1. Crashes never pass.
4. The controller stops and confirms all tracked children and the owned Windows
   job empty before deleting filters. All four exact runtime IDs must be absent,
   the sublayer deleted, and the dynamic session closed successfully.
5. Four fresh positive probes must work again. They are stopped/reaped, and the
   job must be empty before a success result can be written.

The only spawn target is `current_exe()`. Children wait for a pipe handshake until
assigned to a KILL_ON_JOB_CLOSE job. Child stdout/stderr are null, with no pipe
draining deadlock. The internal `--probe tcp4|tcp6|udp4|udp6 PORT` role accepts
only loopback and the fixed marker; it has no WFP or command/target-path API.
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
```

Twenty pure fake-API tests cover four-filter order, partial admission failures,
readback field mismatch, pending-child refusal, deletion/read/close failures,
closed roles, monotonic deadline latching, timeout setup admission, strict
Winsock classification and crash/unknown exit refusal. Readback failures now name
the first mismatched field using a static category. Every field remains checked,
including a final exact equality check; no flags are masked or tolerated.
A failure may include bounded unsigned
`requestedFlags` and `observedFlags` diagnostics from the last actual readback.
The requested flag value remains exactly zero. No path or App-ID blob is emitted,
and the successful nine-field result schema is unchanged. These tests do
not call WFP or create sockets/children. Only the pure plan was executed.
Rustfmt is unavailable in the existing toolchain; no component was installed.

Microsoft references: [dynamic session lifetime](https://learn.microsoft.com/en-us/windows/win32/fwp/object-management),
[app-ID filters](https://learn.microsoft.com/en-us/windows/win32/fwp/permitting-and-blocking-applications-and-users),
[ALE layers](https://learn.microsoft.com/en-us/windows/win32/fwp/ale-layers),
[filter readback](https://learn.microsoft.com/en-us/windows/win32/api/fwpmu/nf-fwpmu-fwpmfiltergetbyid0),
[arbitration](https://learn.microsoft.com/en-us/windows/win32/fwp/filter-arbitration).
