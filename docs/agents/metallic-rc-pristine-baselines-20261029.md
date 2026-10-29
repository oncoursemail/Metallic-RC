# Explicit startup baseline variants

Board 362; branch `codex/rc-pristine-baseline-variants-20261029`, based on PR 100
at `e75046e`. This adds a separate pure v2 planner. The v1 planner and its ten
tests remain unchanged.

`tools/client-lab/pristine-baselines.py` reads one explicitly selected candidate,
capped at 65,536 bytes, and prints a plan. It does not read original client files,
inspect processes, create directories or named objects, call WFP, contact a
service, request UAC, or launch anything. Declared pins remain unverified.

The candidate has schema version 2 and exactly the existing seven top-level
fields. Its four exact original-file pins include UnityPlayer.dll
(`30f4a99c49566b2c5045469b5a39572dd2298adc31f7fe2bc39d8bb8fc9d75bb`,
32,858,656 bytes), alongside the existing executable, GameAssembly and Referee
pins. The planner reuses the fixed sibling v1 module's validated path, owner-lab,
budget, launch-argument and environment logic without mutating that module or
the candidate. It rejects duplicate JSON properties at every object level,
nonfinite constants, unknown fields, ambiguous paths and coerced numeric types.
Selected environment values must be valid text; only their allowed names are
reported. Candidate run paths still appear in the explicit argv and plan.

The default `--variant no-authored-section` plans no section creation, mapping,
initialization or consumer. It leaves vendor objects and services untouched and
does not assert that an existing vendor section is absent.

`--variant experimental-zero-section` instead describes the historical
CREATE_NEW, initially zero-filled `Global\KjMpQrStUvWxYzAb` experiment at
66,060,456 bytes. An existing section must be refused, never modified. There is
no queue-header initializer or consumer. This remains an external dumping
hypothesis, not a verified startup prerequisite, repair or crash cause.
Neither variant falls back to the other.

Both retain `gameWillNotRun:true`, `executionReady:false`,
`newNetworkMethodApproved:false` and `originalBytesVerified:false`. A passing
authored guard fixture does not establish the original target's guard profile,
receipt provenance or an executor. Those gates remain explicit blockers. No
candidate field, ready report or CLI option can grant execution authority.

Future process preparation explicitly permits only three standard-I/O handles:
stdin and the stdout/stderr write ends. The handle list is mandatory; unlisted
handles, the target job handle and parent read ends must not be inherited. This
replaces v1's literal “without inherited handles” wording only in the new plan.
The same closed argv, cwd, twenty environment names including SystemDrive, and
90/30/130-second budgets are preserved.

Checks performed with authored temporary candidates only:

```text
python -I -S -B -m unittest discover -s tools/client-lab -p test_pristine_baselines.py -v
python -I -S -B -m unittest discover -s tools/client-lab -p test_pristine_control.py -v
git diff --check
```

All 13 new test methods and all ten unchanged v1 methods passed. New cases cover
both section variants, four-pin substitution, strict schemas and budgets, path
and Unicode failures, environment privacy, duplicate/oversized JSON, refusal of
`--run` before file access, exact launch arguments and standard-I/O ordering,
and preservation of source inputs and candidate file contents/mtime.

A separate `pristine-baselines-ci.yml` workflow schedules these exact two suites
on Windows and Ubuntu with Python 3.13 and a five-minute job limit. It uses only
read access, disables checkout credential persistence, and has no artifact,
secret or publishing steps. Paths include both planners, both test files and
the workflow itself. Hosted results are pending; local results above used the
existing Python 3.15 runtime.

No original client, named section, guard, service, UAC prompt or native test was
started. This change prepares reviewable variants; it establishes no new runtime
result or causal explanation for the recorded Referee fault.
