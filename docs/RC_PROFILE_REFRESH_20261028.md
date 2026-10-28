# Refreshing a candidate native-test profile

Source candidate, 28 October 2026. No real profile was generated or changed, and
no native process, Frida device, game, account, credential or service was used.

The profile preparation tool previously accepted only schema versions 1 and 2.
An existing schema-3 progress-store profile therefore could not be used as the
verified input to another candidate after separately reviewed source changes.

`tools/client-lab/prepare-manifest.py` now accepts schema 3 only after verifying
every old file's recorded size and SHA-256, rejecting duplicate/ambiguous paths,
and verifying the explicit old runner path and its separate runner hash against
the pinned record. Missing, changed or unpinned old inputs prevent output.
Duplicate JSON properties and malformed pin identities are rejected as well.

Legacy schema-1/2 input remains supported. If it supplies a runner hash without a
runner path, only the pinned `run-native-client.py` beside the admitted agent can
satisfy that hash. No directory search or source execution is used to infer it.

The tool preserves unrelated records and their metadata, including original game
files, vendor files and extra evidence. Files sharing an authored basename in an
unrelated vendor/extra directory are retained. Only the explicitly admitted old
agent/policy/runner and the fixed authored siblings under the old agent/runner
directories are replaced with new source pins. All new source files must be
readable before exclusive output creation; existing outputs and same-path
refreshes cannot overwrite an earlier profile.
Runner metadata uses the same captured hash as its new file-pin record, avoiding
two inconsistent identities if source changes between separate reads. This is
not a filesystem transaction: admission still rechecks current file bytes, so
source changed after capture remains unusable until a new candidate is prepared.

The generated schema, build ID, fixed local port, progress bounds, native-test
budgets and existing canonical scope text remain bounded as before. Old free-text
scope is data and cannot confer stronger permissions. The command receipt marks
the result as a candidate with `approvedForExecution=false` and
`gameLaunched=false`. Profiles are not executed by this tool.

## Offline checks

```text
python -B tools/client-lab/test_profile_refresh.py
```

The new suite uses only newly created synthetic profile files and inert fake
source/vendor/game inputs. It covers v3 refresh, legacy inputs, missing/mismatched
runner identities, changed and missing pins, path aliases, duplicate JSON keys,
invalid pin metadata, wrong build/port/schema, same-path and existing-output
refusals, an exclusive-create race, missing new sources, preservation of extra
records and candidate-only command output. The fake source contents would fail
if executed; hashing does not run them.

Executed locally: **18 new offline tests passed**, and the unchanged startup
output suite passed **14 tests**. The unchanged preparation script from the base
commit was also run against a fully synthetic v3 profile: it exited with the
expected schema rejection and created no output. The private, ignored baseline
receipt contains only that aggregate result. Diff checks passed. Hosted results
and independent review remain separate gates.

This is preparation, not native admission. A later original-client run still
requires the existing independent review, exact-file checks, process ownership,
time budgets and separate approval controls. Active profiles, manifests, pinned
sources and installed services remain unchanged by this source candidate.
