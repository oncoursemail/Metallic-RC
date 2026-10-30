# Controlled uninjected startup observation

This candidate prepares one bounded observation of the fixed installed Rec Room
build 23191908. Default invocation and `--plan` are inert. The source defines two
closed Windows roles:

- Launcher: `--observe-no-authored-section <launcherSha> <guardSha>`.
- Guard, called only by that launcher through normal Windows consent:
  `--guard-observation <nonce> <parentPid> <created> <launcherSha> <guardSha>`.

The parser and Windows admission checks produce a typed `Permit` passed into
the controller. There is no arbitrary executable, path, profile, environment,
command or receipt option. Hashes identify the reviewed sibling binaries;
neither a JSON assertion nor an environment value can grant a different scope.
Malformed roles remain refusals. CI executes inert/refusal tests, never the
complete admitted observation roles.

## Identity, guard and process order

Low and high independently verify their own and peer images, retained file
identity, owner SID, session and process creation time. They also verify four
fixed original images: RecRoom.exe, GameAssembly.dll, Referee.dll and
UnityPlayer.dll. These pins cover four files, not the complete installation.
The target path, screen arguments, working directory and 20-name environment
allowlist come from source; `SystemDrive` is mandatory.

The guard uses the actual fixed RecRoom.exe App-ID. Four exact IPv4/IPv6
connect/receive-accept BLOCK rules must pass live readback before Ready. Only
then may the medium launcher create the suspended target. Actual target image,
file identity, token, SID, session and creation time must match before the high
peer ACK permits one resume. The target receives only three listed stdio handles.
A kill-on-close job with active-process limit one and restricted-child policy
precedes resume. The guard retains only query/synchronize target rights and
never holds the job handle.

`originalAppIdReadbackVerified` describes the installed rule objects. It does
not establish an observed original-client connection or drop, full isolation,
successful startup, login or gameplay. Earlier authored-worker tests cover the
guard mechanism on their own image; they do not supply original-target traffic
evidence. Startup, login and actual original traffic remain unverified here.

## Observation and cleanup

Observation ends at most 80 seconds after normal consent returns, subtracting
startup work already elapsed. The guard adapter retains its 90-second budget;
each process has a 130-second hard ceiling. An input-idle hint is sampled for
at most 30 seconds after resume and is not startup success.

A natural nonzero exit remains failure. A running target is recorded before
owned termination; termination does not become a successful launch result.
Cleanup requires the retained target exit, actual job history of exactly one
process with none active, drained output and verified guard cleanup. Missing
exit, accounting, drain or cleanup evidence stays unverified. The high peer
keeps guards through target-exit/STOPWAIT rather than accepting a PID-only or
ready-JSON claim.

Stdout/stderr retain bounded 65,536-byte prefix fingerprints, never printed raw
contents. The private player log is metadata-monitored with a sampled 4 MiB
stop threshold; a native write can overshoot it. Logs stay private. This path
creates no authored global section, injects no Frida, installs no hook, changes
no TLS/authentication code, loads no external patch, starts no service and takes
no account action. Existing vendor objects and services are untouched; their
absence is not asserted.

Same-name process checks are snapshots, not atomic exclusion. Another process
can start afterward, and App-ID rules can affect independently started copies
of that image. Existing-service delegation, DNS/service mediation and foreign
process lifetimes remain outside the target job's scope. Creation-to-job
assignment, hard teardown, owner death and dynamic-filter release have
non-atomic gaps. This controlled baseline is not equivalent to an ordinary
Steam launch.

## Review checkpoint

The observation roles for this source/artifact have not run at this checkpoint.
The owner expects one bounded observation after independent source review,
fresh hosted CI and an exact artifact gate, retaining normal Windows consent.
That is not blanket authority for automatic repeated execution or other targets.

The crate reuses the reviewed sibling protocol, PE, IPC and WFP implementations
and the external fixture's exact rule/readback/cleanup policy. Its source checks
cover the fixed role parser, pure protocol/PE rules, inert/refusal entrypoints
and data-only Windows guard PE inspection. No passing source test or rule
readback establishes original-client startup or backend compatibility.
