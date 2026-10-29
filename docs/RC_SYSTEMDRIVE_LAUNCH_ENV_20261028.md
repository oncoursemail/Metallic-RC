# Preserve SystemDrive in the native-test environment

Source candidate based on `4ef4c09d837106fc700e038d5e749fabba0961ac`.
The runner's child-environment allowlist now passes through `SystemDrive` when
it exists in the parent. It still constructs a closed environment dictionary;
it does not inherit arbitrary variables or synthesize a drive value.

## Reproduced Windows API defect

An independent, read-only Windows control called
`SHGetKnownFolderPath(FOLDERID_ProgramData, flags=0)` under six environment
cases. It created no files and executed no original client:

| Environment | HRESULT | Returned path |
|---|---|---|
| Full | `0x00000000` | ProgramData |
| Exact old runner allowlist | `0x80070003` | Null |
| Old allowlist plus SystemDrive | `0x00000000` | ProgramData |
| Old allowlist plus ALLUSERSPROFILE | `0x80070003` | Null |
| Old allowlist plus both variables | `0x00000000` | ProgramData |
| Full environment minus SystemDrive | `0x80070003` | Null |

These OS controls ran without elevation; the approved original-client test ran
elevated. No elevated control was attempted. Independent replay reproduced all
six results while preserving the probe, report and baseline-runner hashes.
The API and folder identifiers follow Microsoft's [Known Folder API](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath)
and [folder identifier documentation](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid).

The independently retained aggregate is schema
`known-folder-environment-control-v1`, SHA-256
`01a630b3a3ae0cbb1028fec28f9e873d7eaa8b09be9c8dd40eef8984d5eb7a95`.
Its authored probe SHA-256 is
`10c727216812b9a3d2667b056c087f9460a7bdd313fd2f95a9ee97c0fa5dd2e2`.
The control identified the old runner as SHA-256
`3963fa76b457c26c1c43f9e60885167657d82186690bc565d99c3864fc4fb2c4`.
No private log contents, environment values, account data or vendor code are
included in this note.

Static analysis separately identified a Release-launcher path through the
ProgramData known folder to a Referee launcher log. Its traced title/message
differs from the owner's photographed Referee Error dialog. The missing variable
is therefore a reproduced launch-environment defect, **not a confirmed complete
explanation of that dialog**. A later separately authorized native run advanced
through attachment and resume, then crashed before discovery; details follow.

## Authorized native result, 29 October

One owner-authorized run used candidate manifest SHA-256
`b46b58e23819a0dc01bae8be6f33d025b583f38a417a2dd4e9889b16a1d6876b`
and runner SHA-256
`9f175658ba57321a8ac393addf39ca5f811cad39e172998011f94302c60aa144`.
The unchanged controller required normal Windows UAC approval. Attachment
completed in 6.766 seconds; the agent loaded, installed its network guards,
verified five method identities and resumed the new owned process. This is
progress beyond the earlier attach timeout, not proof of complete error recovery.

After 37.014 seconds of observation the client exited with `0xC0000005`.
PID-correlated Windows Application events identify `frida-agent.dll` at RVA
`0x133d9e`. The reported instruction is in an exception-handler prologue and
may describe a secondary failure; no original trigger, register state or stack
trace was recovered. No native HTTP, URI-routing or discovery event was captured.
The legacy `runtimeRoutingHookUsed` receipt flag records adapter readiness,
not proof that a request was rewritten. An absent event does not prove that a
callback was never entered before the fault.

The supervisor finished in 46.136 seconds with result 2, an empty owned job,
and both original executable hashes unchanged. Native discovery, login,
gameplay, voice and successful log initialization remain unverified. No automatic
native retry followed. Private logs and crash artifacts are retained locally;
they are not release attachments.

## Scope and checks

The production change adds exactly one allowlisted variable. Executable/argv,
game working directory, absolute private Unity log path, `stdio='pipe'`, UAC,
ownership checks, routing/network guards, deadlines and cleanup are unchanged.
There are no installation, ACL, service, protection or authentication changes.

A focused fake-worker test supplies a synthetic parent drive and an unapproved
sentinel. It observes the exact drive value reaching the child configuration and
the sentinel remaining absent. The test failed against the old runner because
the drive was missing, then passed after the allowlist change. Shared fake-spawn
assertions also retain executable, screen arguments, working directory, one
absolute private log argument and piped output. All native/instrumentation
operations remain fakes in this suite.

Executed locally: **15 startup-output/fake-worker tests** and **18 unchanged
profile-refresh tests** passed; `git diff --check` passed. The refresh suite uses
only newly generated inert temporary fixtures. It did not generate an active
replacement profile.
The parent independently reran those 15 and 18 checks. A separate real Windows
fixture then spawned only an authored Python child, attached the pinned Frida
runtime, installed the existing guards and cleaned its owned job. Native
cancellation completed in 0.203 seconds; the owned health request succeeded,
while the fixture's reserved external address, other local port and invalid DNS
request were blocked with the expected codes. No game was launched. Its private
result SHA-256 is `dc72269f6669dba59794c0ad2b55947b754813b4a7181a0bcf8c5fbe9a4aca1f`.
These scoped guard observations are not a Windows firewall or proof covering
every possible networking API.
The broader Ruff check reports the same 21 existing diagnostics in the baseline
and candidate files, with no new diagnostic signatures.

```text
python -B tools/client-lab/test_startup_output.py
python -B tools/client-lab/test_profile_refresh.py
```

The previous source checkout and reviewed candidate remain unchanged. Any later
native attempt requires a newly prepared exact-hash candidate and the existing
separate review and owner authorization. This source candidate neither launches
the original client nor claims a successful native login or discovery.
