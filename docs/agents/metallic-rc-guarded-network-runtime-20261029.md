# Medium-worker network adapter candidate

This source extends the authored-worker launcher from PR102 and the pure MGM2
contract from PR103. It does not add an original-client target. Explicit
`--network-control` and `--network-one-shot` dispatch only to the fixed authored
worker and verified peers. The default and `--plan` remain inert. Source, CI and
artifact admission precede any guarded run; normal Windows consent is preserved.
The separate non-admin `--network-self-test` runs four fixed loopback workers
only; it cannot start a high helper or open WFP.

The before-controls precede consent. After normal Windows consent, a pinned
medium launcher and high helper authenticate the retained process identity and
OS pipe peer. Hello uses the existing identity frame; the remaining messages
are exactly 128 bytes. Ready is sequence4; each of four probes has Register,
Ack, Result, ProbeDone and Next; Release/Released are25/26. Every transition
binds nonce, kind, sequence, phase, protocol and retained root/port. The helper
retains all registered roots and rejects reused PIDs, earlier creation times
and already-exited targets before Ack. Low resumes each suspended root once.

Low owns job cleanup and a single reader per output stream. UDP metadata is
available only for exactly32 output bytes and EOF, then decoded as the existing
MRU1 record. Natural pre-cleanup exit, post-cleanup agreement, job emptiness and
both stream EOFs are mandatory. Forced termination never becomes an accepted
network observation. Each listener is created after Ready, near its own probe;
none waits through UAC. Endpoints/payloads are fixed loopback and four-byte MRC1.

High compares the reported code with the retained process's actual exit. A UDP
timeout needs the actual native outbound classify-drop event, not reconstructed
event fields derived from a count. The shared observer returns its first matched
sanitized event only after complete enumeration, successful enum cleanup and
budget checks. API failures, caps, incomplete results and deadlines discard the
sample. The existing fixture's count-only observer keeps its prior behavior.
The shared pure blocked-outcome predicate checks the current CONNECT rule/layer,
App-ID, full oriented tuple, family/protocol, presence bits, direction and UTC
window. Inbound traffic at the launcher's listener is never worker proof.

Low trusts ProbeDone and Released from its authenticated pinned high peer. It
does not claim independent low-side BFE inspection or invent a GuardScope/Event
from wire data. The full pure Planner remains a model; the split executable
adapters enforce their own sender-specific sequence, binding and deadline checks
and reuse its blocked-outcome predicate. Host assertions remain code authority,
not permissions obtained from JSON or a model-written receipt.

Before release, high-side failure retains the engine until launcher exit AND
every registered root's exit; this covers an unknown next suspended child. Low
cleans its owned job/listener and exits on error, without waiting for a helper
that is itself awaiting launcher exit. After four accepted results and cleanup
confirmations, Release allows filter deletion/absence verification and session
close. Failure during/after that attempt is recorded separately; it does not
pretend that deleted guards remain installed. Low runs after-controls only
after authenticated Released and actual successful helper exit.

Protected startup/total checks are30/90 seconds after consent; hard process exit
is130 seconds. Calls already in progress may overrun a soft check. Forced helper
death, job assignment and dynamic filter removal are not atomic containment.
Existing-service delegation and other executable identities remain outside this
fixture. Successful authored proof will not establish Rec Room readiness.

Local preparation checks currently pass:43 serial library checks, including
six actual same-process pipe controls and owned TCP4/TCP6 exchanges;41 external
fixture policy/observer checks; two actual image metadata checks; a locked
release build without warnings. Native
cross-integrity blocked probes are not yet run for this candidate. Independent
source/workflow and closed-dispatch reviews passed; fresh hosted CI and final
artifact verification are required before guarded execution.

The initial actual positive-worker run failed its strict one-process accounting
check despite natural exit0 and verified cleanup/EOF. A private diagnostic of
the fixed Wait worker observed a second owned job member named conhost.exe;
that basename was not treated as system-image authentication. The diagnostic's
later TCP observation failed and is not counted as a pass. The authored worker
now explicitly links as a Windows GUI-subsystem binary, with a data-only PE
check, so it can use the supplied pipes without automatic console attachment.
The one-process check is unchanged. The new pinned four-worker run passed all
TCP4/TCP6/UDP4/UDP6 probes, followed by all ten prior worker controls; every
source/artifact pin stayed unchanged during independent review. The original
failure and all old artifacts remain preserved. These positive controls do not
establish blocked traffic or original-game behavior.

References: [Rust subsystem attribute](https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute),
[Windows console creation](https://learn.microsoft.com/en-us/windows/console/creation-of-a-console).
