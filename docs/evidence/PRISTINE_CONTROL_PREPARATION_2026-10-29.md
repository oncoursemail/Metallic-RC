# Direct-executable control preparation

Board claim 338. Branch `codex/rc-pristine-control-20261029`, based on `c41f496`.

`tools/client-lab/pristine-control.py` validates candidate data and prints a plan. It contains no original-client executor, Frida import, subprocess launch, WFP call, service operation, elevation request, registry change or profile writer. `--run`, abbreviated flags and unknown arguments are rejected before input is opened. The CLI reads only the explicitly selected JSON candidate, capped at 65,536 bytes, and prints JSON to stdout.

Every plan states `gameWillNotRun: true`, `executionReady: false`, `newNetworkMethodApproved: false` and `originalBytesVerified: false`. Blocking reasons include the unverified external fixture, missing native executor and unverified original bytes. Candidate booleans cannot remove these blockers. A later fixture would need independently reviewed exact source identity and actual passing receipt provenance; this planner does not consume or approve such evidence.

The closed candidate schema contains `schemaVersion` (integer 1), `steamBuildId` (`23191908`), `gameRoot`, `labRoot`, `runDirectory`, `files` and `budgets`. The game path must match the fixed source identity. The expected lab is derived only from the provided Windows `LOCALAPPDATA` environment value plus the fixed `Metallic-RC/native-client-lab-20261021` suffix; missing, empty or invalid values are refused without a fallback. Environment names are matched case-insensitively. No owner-specific lab path is embedded. The run directory must be one named child of the fixed lab's `runs` directory. Relative, UNC, parent-traversing, alternate-stream and reserved run names are refused. A future executor must still create a new private directory, reject existing/reparsed paths, apply the owner-only evidence ACL and verify current file identities. This pure path validation does not inspect the filesystem or establish those admissions.

Exactly three pin declarations are required: the reviewed `RecRoom.exe`, `GameAssembly.dll` and `Referee.dll` identities and lengths. Changed, missing, duplicate, extra or substituted declarations are refused. No original file is opened or hashed by this planner; pin declarations are not current-byte verification. The existing manifest refresher and approved profiles remain unchanged.

The plan retains the direct executable, existing screen-mode argv, game-directory cwd and 20-name environment allowlist, including `SystemDrive`. Environment selection preserves only existing allowed values, handles Windows name casing, refuses ambiguous duplicate names, and emits names rather than values. It retains the reviewed create-new shared-section prerequisite as a future requirement. This is a harness control design, not evidence that normal Steam or Release-launcher startup works.

Required order is explicit: verify source/original bytes and refuse existing processes; create private outputs and the section; install and verify separately approved external guards; create the target suspended without inherited handles; retain the exact image and verify kill-on-close job membership; then resume. The total worker budget is 90 seconds, startup limit 30 seconds and outer supervisor wait 130 seconds. There is no attachment, so no 15-second attachment check is reported as passed. Pre-cleanup exit evidence precedes close or termination. The owned job must be empty before guards are released, followed by original-hash checks and final receipt export. Raw player logs remain private; stdout/stderr require bounded per-stream fingerprints and transactional progress/final evidence.

External app-ID filtering does not automatically cover other executable paths, helpers or delegated services. No service start or installer execution is part of this plan. Dynamic-filter removal and forced controller/job cleanup are not claimed atomic. These are limits of the proposed experiment, not claims that external containment is already implemented or verified. Microsoft's [application-ID condition](https://learn.microsoft.com/en-us/windows/win32/fwp/filtering-condition-identifiers-) and [dynamic-session lifecycle](https://learn.microsoft.com/en-us/windows/win32/fwp/object-management) describe those underlying scopes.

Validation:

```powershell
python -B tools/client-lab/test_pristine_control.py
git diff --check
```

All 10 authored test methods passed, including missing/invalid `LOCALAPPDATA`, case-insensitive lookup and mismatched-owner lab rejection. They cover exact argv and pin declarations, path and budget rejection, environment selection without value disclosure, guard/image/job/resume/cleanup ordering, refusal of input authority claims, bounded/duplicate JSON, and rejection of execution flags before file access. The pure planner is tested with file opens forbidden; the CLI uses only temporary authored candidate data. No actual candidate profile, original-client process, external fixture, WFP session, service, account or UAC operation was created or executed.

Subsequent diagnostic preparation (board 339): an independently arriving failure
artifact contains `readback_mismatch`; its invoking process exit status is not
known. It is preserved as diagnostic data, never admitted as a passing receipt.
The fixture now reports static first-field mismatch categories for missing
readback, ID, key, sublayer, layer, App-ID equality, action, flags, condition count
and match type. Native pointer/type checks also have distinct static categories.
Exact acceptance, filter policy, admission ordering and cleanup are unchanged,
with a final whole-rule equality check retained. There is no flag mask or inferred
normalization. All 18 pure tests and the offline locked Windows Release build
pass, with independent review. Only the inert plan was executed for this revised
binary during preparation. The actual mismatched field is not yet established.

The next controlled authored-fixture run identified `readback_flags_mismatch`
and returned failure. It did not establish the returned numeric flags or verify
blocking/cleanup. A subsequent diagnostic-only revision records that unsigned
value in the failure result, when an actual readback exists, alongside the
unchanged requested zero. No observed value changes the policy, and the successful
nine-field schema is untouched. All 20 pure tests and the offline locked Windows
Release build pass. These checks do not establish the actual returned value;
that remains a native-test question.
