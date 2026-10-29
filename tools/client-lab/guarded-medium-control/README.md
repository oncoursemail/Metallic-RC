# Authored guarded-medium lifecycle preparation

Board 361, phase 1. This is an isolated prerequisite with three fixed sibling
binaries: metallic-medium-control.exe, metallic-medium-guard.exe and
metallic-medium-worker.exe. Default/no arguments and --plan do not start either
helper or target. No original-client role or authored shared-section profile is
implemented. Target-specific network proof, execution readiness and full sandbox
claims remain false.

## Scope and state

The explicit source-only control role keeps the launcher at medium integrity.
It accepts only three reviewed-build SHA-256 values as identity data, never an
executable path or arbitrary command. These values are not independent
attestation or an elevation grant. Each image and directory ancestry remains
read-locked, rejects reparse components, and is checked against retained process
file identity. The launcher validates the guard's real PE load-configuration flag
before the sole ShellExecuteExW runas call. This preserves normal Windows UAC;
there is no alternate elevation path or persistent broker. The medium deadline
starts after the consent call returns a retained helper handle. Time spent at
the consent dialog does not consume the protected-worker budget.

Only the one-shot high guard calls WFP. Its target is the fixed sibling authored
worker, with exactly the four reviewed App-ID filters, INDEXED-only flags and
live rule readback through the unchanged fixture policy dependency. It cannot
launch a process, select another path or command, modify services or grant BFE
permissions. If medium cannot independently read BFE, no ACL is changed: the
trusted pinned high peer's exact authenticated Ready receipt is the stated
readback authority. This phase proves no four-protocol network behavior for the
medium worker; it only prepares the lifecycle around a marker worker.

The local named pipe has an owner-only DACL, FIRST_PIPE_INSTANCE,
REJECT_REMOTE_CLIENTS, message mode, a fresh 128-bit CNG nonce and closed 64-byte
frames. Both endpoints check OS pipe peer PID against their retained peer's PID,
creation time, locked image/file ID/hash, same SID/session and expected
medium/high token elevation and integrity. The high client opens with
SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION, preventing the medium server
from impersonating it at an impersonation-capable level. The nonce is not the
authentication boundary and is not logged as evidence. No ready file is trusted.

Hello precedes four-rule installation/readback; Ready precedes creation of the
suspended target. The target has an explicit 20-name Unicode environment, only
three selected standard handles, noninherited pipe read ends, bounded concurrent
stdout/stderr drains and immediate retained cleanup ownership. Its kill-on-close
job has no breakaway and remains medium-owned. The high guard never receives a
job handle. It opens only query/synchronize target access, verifies the target
stamp/image/token, and retains that real handle before sending Ack. Only the
exact nonce/sequence/target Ack permits one resume. Normal Done requires the
medium-owned job to be empty, completed drains and the marker's observed exit
zero; high also waits the retained target root and checks its actual exit zero
before rule cleanup and Released.

After a post-Ready failure or lost Ack, high retains guards while the registered
target can still be alive. If registration was lost, it waits for the launcher
instead. Query errors and soft timeouts stay in STOPWAIT; they never cause an
early Native drop. Cancellation before target creation is failure, followed by
launcher-exit observation. The registered-target path never substitutes launcher
exit for target exit.

## Explicit limits

The independent 130-second hard process deadline is a final unverified failure
boundary. It can tear down a dynamic WFP session even if a target/launcher exit
could not be verified. The authored worker has its own 120-second deadline, but
this is not proof of atomic guard lifetime or adversarial containment. There is
also a suspended creation-to-job-assignment interval; abrupt launcher death
there is not proved atomic. Failure receipts retain cleanupVerified false.

Overlapped pipe operations use retained buffers/events/handles, fixed finite
waits and CancelIoEx. If cancellation completion cannot be confirmed, the
kernel-referenced operation allocation is deliberately retained until process
teardown; it is not freed while pending or counted as clean cancellation.
Normal output, filesystem paths, SIDs, environment values and App-ID blobs are
not printed. Create-new private receipt files report only closed status/numeric
facts and bounded output hashes. These are trusted-local-state diagnostics, not
tamper-proof evidence against another process with the same user's authority.

Ordinary job cleanup does not prove delegated DNS, RPC/proxy/service or helper
coverage. This phase does not restrict all child creation, inventory every
Windows helper, execute an original client, install a service, change global
network logging, alter firewall/BFE ACLs, connect an account or broadcast.

## Loader admission

The MSVC guard build requests /DEPENDENTLOADFLAG:0x800. The medium launcher
requires the actual file-backed PE64 LoadConfig DependentLoadFlags to equal
0x800 before UAC; zero, extra flags, missing/ambiguous sections and out-of-bounds
metadata refuse. Every section intersecting any of the 80 trusted bytes is checked, including partial virtual overlaps and raw-file aliases. The high guard sets process-local
SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) on entry and fails closed
on API failure. A read-only version check before UAC requires the documented
Windows 10 RS1+ platform floor (version 10.0, build >= 14393); unknown version
families refuse. This controls dependency search, not arbitrary DLL behavior.
Microsoft's existing SDK dumpbin independently reported 0800 Dependent Load Flag
for the actual built guard.

Primary references: [static dependency search flags](https://learn.microsoft.com/en-us/cpp/build/reference/dependentloadflag),
[process-local DLL search](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-setdefaultdlldirectories),
[named-pipe security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights),
[overlapped cancellation](https://learn.microsoft.com/en-us/windows/win32/fileio/canceling-pending-i-o-operations).

## What actually ran

The locked offline release build passed without warnings. Fourteen pure tests
passed for closed frames, identity-fact rejection, nonce/PID/time spoofing,
ready/register/ack/resume ordering, cancellation/lost-Ack/parent-death decisions,
STOPWAIT on unknown/live targets, PE metadata and the environment/pin rules.
These are model/unit checks, not proof of actual cross-integrity pipe security.
One data-only integration test inspected the actual release guard's PE and
rejected four in-memory flag mutants; it did not execute the guard. Ten actual
non-admin authored worker controls passed: nine copied from the separately reviewed
adapter, plus a new retained-worker identity check. The copied controls cover wrong pin, suspended silence/read lock/double resume, environment scrub,
concurrent output bounds/hashes, unresumed cleanup, descendant timeout cleanup,
single-worker parent death and first/second drain-start failures. The new check
verifies the actual fresh suspended worker using query/synchronize handles and
rejects wrong PID, creation time, SID, session, elevation and image before
confirming unresumed empty-job/EOF cleanup. No UAC,
--authored-control, --one-shot, WFP or original-game role was invoked.

Only these nonprivileged checks are preparation commands:

```powershell
cargo build --offline --locked --release --bins
cargo test --offline --locked --lib -- --test-threads=1
cargo test --offline --locked --test guard_pe
.\target\release\metallic-medium-control.exe --plan
.\target\release\metallic-medium-control.exe --self-test $reviewedWorkerSha256
```

The privileged pair and cross-integrity peer verification/cleanup remain unverified until independent source review and a separately recorded
native test. No binary or private receipt belongs in source publication.

## Additional transport and PE review checks

A new regression first reproduced the PE parser accepting a second section
mapping only part of the trusted LoadConfig. The fix checks every intersecting
virtual/raw segment before accepting full 80-byte coverage. Partial mappings of
just the flag bytes, virtual-only overlaps and raw-file aliases refuse; a separate
nonoverlapping section still passes. The actual built guard PE check remains
required after each release build.

Five real same-process Windows pipe tests passed locally with the observed token
not elevated: delayed-writer roundtrip and actual OS peer PIDs, short/extra/bad
messages, EOF, confirmed read cancellation followed by an intact next message,
and confirmed connect cancellation. Cancellation waits verify completed aborted
I/O; pending/unconfirmed contexts remain retained, never freed. The transport
tests do not require elevation or a non-admin token, so an elevated hosted run
would test transport honestly without claiming medium/high isolation. Production
launcher and target non-admin gates remain unchanged.

Initial transport resource assertions failed on a process-wide handle count of
112 to 113; the first failure poisoned the test mutex and the remaining four
controls were not passes. Isolated repeated cancellations showed the first +1
followed by five stable counts, with zero live operation contexts. A no-pipe idle
control stayed unchanged. The owner of the initial +1 remains UNATTRIBUTED.
Final controls perform one bounded completed pending-I/O prime before resource
measurement and retain strict no-growth assertions during measured work.
Test-only accounting additionally requires every wrapper-created token, pipe,
event and duplicate handle to close successfully, zero live owned handles, and
zero close failures; no tolerance is added for owned leaks. Reader threads join
before resource checks. These observations are not a universal process-leak or
cross-integrity security proof.

After high token/self-image admission and authenticated caller admission,
pre-Ready failures finalize their owner-only private receipt with a static error
and stage (for example, filter_install). Caller admission includes the retained
pinned launcher stamp/image, same SID/session, expected medium token and actual
pipe-server PID. It precedes every high diagnostic directory/file creation.
Failures in that preauthentication path exit without a private receipt; stderr
and nonzero exit are not success. Later worker/installation failures are recorded.
This adds no ready-file authority or arbitrary output path. The post-Ready
STOPWAIT lifetime remains inside the same guard scope; labeling a failure does
not drop live guards early.

Focused transport command (no guard or WFP role):

```powershell
cargo test --offline --locked --lib ipc::tests -- --test-threads=1
```

On Windows, the full library test command now includes 14 pure checks and these
five actual pipe controls plus the profile-root regression, for 20 tests. The data-only actual-PE check and ten
non-admin worker controls remain separate. None of these invokes UAC, WFP or the
privileged pair.

## Token-derived output root

Review found that trusting LOCALAPPDATA could redirect elevated output creation.
The new non-admin regression reproduced that baseline defect using only an
authored temporary directory. Production output now opens the current process
token with TOKEN_QUERY and calls GetUserProfileDirectoryW with a bounded Unicode
buffer, then appends exactly AppData/Local/Metallic-RC/guarded-medium-control-checks.
It never reads LOCALAPPDATA or USERPROFILE to choose the output root. Existing
local-drive/no-reparse ancestor locks, owner-only new directories and create-new
files remain. Unsupported/redirection layouts fail closed; this is the fixed
profile-relative suffix, not an arbitrary known-folder or environment fallback.

The actual regression passed with the observed local token not elevated: it
sets both LOCALAPPDATA and USERPROFILE to an authored empty temporary directory,
creates/writes a synthetic output only under the token-derived root, confirms
the spoof directory remains untouched, cleans its owned leaf, and restores the
original environment through RAII even on failure. It never tests a protected
folder. Tests run serially because this regression changes process environment.
An elevated hosted test can exercise its own token profile without impersonation
or a guard role; that is not proof of high/medium pipe isolation.

The admitted high guard now verifies caller ownership/session/image and OS pipe
peer before creating its private output. No BFE ACL, profile registry entry,
privilege, service or machine setting is changed. Old private staged images were
not executed and are not approval for this source revision.

Primary API: [GetUserProfileDirectoryW](https://learn.microsoft.com/en-us/windows/win32/api/userenv/nf-userenv-getuserprofiledirectoryw).
