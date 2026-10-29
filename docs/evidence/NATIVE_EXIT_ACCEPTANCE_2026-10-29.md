# Native exit acceptance correction

Board claim 334. Candidate branch `codex/rc-referee-startup-20261029`, based on `05e9b38`.

The owned test runner could record a valid discovery event and then return success even though the client had already exited with a nonzero status. Two regressions reproduced this before production changes: the existing full-mode worker fixture returned `0` despite its retained process reporting `0xC0000005`, and the supervisor accepted a complete discovery receipt containing the same pre-cleanup exit code. Both new expected-failure assertions failed with `0 != 1` on the baseline.

The worker now records `clientExitCodeBeforeCleanup` from its retained process after collecting pre-cleanup diagnostics and before requesting close or termination. A nonzero exit observed there, or in those diagnostics, changes an otherwise successful/no-discovery result to failure `1`. It retains `nativeDiscoveryParsed` as the observed event fact. Unavailable or malformed pre-cleanup diagnostics cannot establish success. The supervisor independently checks those observations before accepting result `0`, rejecting incomplete receipts from an older worker as well. Both the explicit retained-handle poll and diagnostics keys must be present before result `0` is accepted.

A process still alive before deliberate cleanup may later receive cleanup termination code `124`; that cleanup result is not treated as an earlier crash. A genuine early zero exit with valid discovery proof retains the existing limited discovery result. A live network-only/no-discovery run remains result `2`. Existing worker timeout `124` remains a timeout. Tests also cover a process exiting between observation and final pre-cleanup capture.

Validation uses authored fake Frida/process objects and the existing owned Python-child containment tests. No original client, Referee component, account, elevation, or new native profile is used. No target path, command argument, environment allowlist, guard code, UAC control, original-file pin, or time limit changed.

This corrects result reporting; it does not repair or explain the recorded vendor-component crash. Discovery proof remains separate from login, room joining, gameplay, or stable startup.

Checks on this candidate:

```powershell
python -B -m unittest discover -s tools/client-lab -p 'test_*.py' -v
node --test tools/client-lab/route-policy.test.js tools/client-lab/runtime-agent.test.js tools/client-lab/runtime-readiness.test.js tools/client-lab/runtime-method.test.js tools/client-lab/runtime-client-mode.test.js
git diff --check
```

All 87 Python unittest methods passed in 31.592 seconds; all 87 JavaScript tests passed. The Python total includes 14 adapter-mode checks and 6 acceptance checks. Diff whitespace validation passed. Both baseline failures were observed before modifying the two production files.

After independent review required the explicit poll key, the 6 acceptance methods and 14 adapter methods passed again, including the new missing-poll-key assertion. JavaScript and all other source remained unchanged.

The parent reran all 87 Python methods after that final adjustment (26.058 seconds, pass). A separate peer independently reproduced both baseline false successes and reran all 20 focused adapter/acceptance methods successfully. Original-client crash analysis is documented separately in [the offline Referee evidence note](REFEREE_OFFLINE_FAULT_2026-10-29.md); it establishes no vendor-component repair.
