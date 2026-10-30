# Default-off native localhost gateway

Claim #373, branch `codex/rc-native-gateway-20261029`, based on explicit profile
commit `4ed8d4e5b36342f7b8f4013579474e1127faca45` (#107).

This connects two closed owned-backend operations: manual-password POST
`/recnet/auth/connect/token` and signed GET `/recnet/accounts/account/me`.
It is not a complete RecNet server or original-client acceptance. Refresh,
security-code, 2FA and login-context wire flows remain unsupported. Other
native routes retain their existing 501 behavior.

All four embedding options must be explicit: identity mapping, native
sessions, explicit native profiles, and `EnableNativeGateway`. The CLI never
enables the gateway. Existing services/data are not activated or migrated by
this source change. Activation requires an already initialized, validated v4
database before any earlier migration can run. The v4-to-v5 transaction creates
a dedicated master-key table and a generation-bound request-key table/index.
Default-off v4 remains v4; previously enabled v5 opens off with native state
validation and old owned APIs, while native gateway routes stay inactive.

## Credentials, state and export

The handler uses the reviewed strict password-form decoder. Context values,
including any client_secret, are not authentication authority. Only registered
Metallic-owned credentials are checked; no production service or password is
used. A pre-existing exact native identity and current explicit 20-field
profile are mandatory. Missing or stale state never becomes an invented profile.
Credential lookup/password checking, identity/profile admission and both writes
share one connection/transaction. Existing owned login retains its prior lock
behavior while sharing the same credential-reader and password-check helpers.

The retained method149472 (RVA0x94de560, evidence
`ev_89d0884cfa3fe4197dbb8b895832379828a5156a3c8fde53f66772ef8bb72119`)
conditionally emits `require_password=true` or omits that field. This slice
admits those two cases and always verifies the supplied password. Other values,
duplicates, unknown fields and challenge/context continuations do not silently
weaken authentication. This is a bounded static branch finding, not proof of
the complete native asynchronous login flow.

The token producer uses explicit Metallic-owned JWT policy, not a recovered
vendor algorithm. Login creates exact access/refresh hashes and the independent
32-byte request key in the SAME SQLite transaction. Only after commit does
`WriteTransportBody` export the canonical response. A failed commit cannot
export successful credentials. Delivery failure after commit remains an unknown
client outcome; it does not prove a completed login or undo committed state.
Cancellation is checked after body EOF, before synchronous credential work and
immediately before commit. Disconnect can still race the final check/commit or
delivery; this is not an atomic disconnect guarantee. Bounded read/copy/backing
buffers are cleared, with no claim that managed credential strings are erased.

## Account request eligibility

Identity is resolved from the exact stored issued token, never decoded JWT
claims. The same transaction verifies its expiry/revocation/durable identity,
generation-bound key, signature and current profile. Direct session rotation
without a wire-key update leaves stale key data preserved but unusable by this
gateway; there is no old-key fallback.

Only the exact ASCII raw path is eligible, optionally followed by a bounded
ASCII query. The ASP.NET RawTarget supplies the signed spelling; no decoded
path or reconstructed URI is substituted. Query bytes are signed verbatim and
semantically ignored. Absolute-form targets, encoded path aliases, fragments,
backslashes and control/space characters are outside the slice. GET must have
no body according to the actual body-detection/Content-Length/Transfer-Encoding
features. Exactly one Bearer header and canonical X-RNSIG are required.
Malformed query percent escapes are refused without normalizing valid query
bytes. Token POST permits only its exact raw path, without query/path aliases.

The request signature is the reviewed HMAC profile over the final raw path/query
with no body, compared using fixed-time digest comparison. No arbitrary method
or sampled-body authentication is enabled. The broader historical signer does
not provide full-body integrity above its sampling threshold. A GET signature
is not a general write or request-replay authorization mechanism.

## Secret boundary and limits

The dedicated CSPRNG JWT master is separate from admin/vendor/request keys.
The master and generation-bound request keys are PLAINTEXT secret BLOBs inside
the existing owner-only SQLite directory and consistent backup boundary.
This is not encryption at rest or protection from a malicious local DB admin,
stolen keys or process-memory inspection. Token/refresh plaintext is not stored.
Key copies are cleared on exit; managed strings and all runtime copies are not
claimed universally erased. Neither errors nor metadata export keys or tokens.

Existing session lifetime, family/rotation/history caps and profile limits
remain. New key rows are bounded by the existing family cap. Schema, triggers,
foreign keys, types/lengths, master/request separation and generation metadata
are checked; unavailable or stale keys refuse rather than fabricate success.

## Source provenance and validation

Password form source is copied byte-identically from reviewed commit e5ad06a,
SHA256 `7d2a87901293bbbac23e143e0059c4e5976fb336cbad1dc3cc9cf09595060e51`.
Request signer source is copied byte-identically from reviewed commit 3a6b8de,
SHA256 `42cf11d75315e51109b5dfa24eaead2a9082e909cfa6f1a7a3179d422bcef677`.
Token producer source is copied byte-identically from reviewed commit
`fc20f767904f716aa94bb77e415246f6d75d7f14`, SHA256
`b2c84fdda86aefb29e0a9d9cdf59031acb2aec66c85df4c91507db6f92ee9cc6`.
Existing parser/writer fixtures are preserved.

The new test project uses real ephemeral loopback HTTP, generated temporary
SQLite stores and synthetic accounts/material. The locked Release build passed
with zero warnings/errors, and the corrected suite passed 198 checks/67 requests.
The unchanged credential, session and profile suites passed 134, 122 and 143
checks respectively against this candidate.
It covers successful login/signed account lookup, exact raw-query signatures,
rejected forms/headers/bodies/aliases, rollback, restart/backup, expiry,
revocation and stale profile/key-generation refusal.

Retained raw TCP POST controls use an isolated v4 store clone. Encoded-path,
query, absolute-form, trailing-slash, fragment and malformed-query targets
receive 501. Duplicate Content-Type receives 501; conflicting Content-Length
and Transfer-Encoding or duplicate Content-Length receive Kestrel's 400.
Each rejection leaves both session and request-key tables empty. The exact raw
POST positive control returns 200 and commits exactly one of each row.

Actual HTTP/2 checks use a separate ephemeral h2c-only test host invoking the
same handlers and assert response version 2; bodyless GET/form POST succeed and
body-bearing GET refuses. This does not change or prove HTTP/2 negotiation on
the production application's existing listener. A real slowly supplied HTTP/1
body receives the bounded native timeout envelope; a disconnected partial body
and deterministic cancellation-at-EOF create no credential state.

The first expanded 196-check run passed locally, but its independent rerun
failed the disconnected-body completion wait and produced no success report.
That control had closed a socket immediately after writing, allowing reset
before server admission. The corrected control first requires a complete,
bounded `100 Continue` response, then sends a partial body and disconnects.
It waits for the slow control to complete before capturing its own counter
baseline and retains the three-second completion and zero-state checks.
This test repair establishes admission; it does not diagnose a production fault.

Test-only SQLite callbacks in the dedicated verifier process observe staged
session/history writes, then deny the later key INSERT with actual SQLITE_AUTH.
The handler returns the native 503 envelope and the whole transaction rolls
back. A separate cancellation control observes all staged writes but cancels
before commit and likewise leaves no state. No production hooks, extension
files or schema triggers are used. Callback lifetime/exception containment and
owned unregister are explicit. These are authored SQLite fixture callbacks,
not original-client hooks; they prove no disk-commit timing or game behavior.
Primary APIs: [auto-extension](https://www.sqlite.org/c3ref/auto_extension.html),
[authorizer](https://www.sqlite.org/c3ref/set_authorizer.html),
[update hook](https://www.sqlite.org/c3ref/update_hook.html).

```powershell
dotnet build tests/Metallic.RC.NativeGateway.Tests/Metallic.RC.NativeGateway.Tests.csproj -c Release -p:RestoreLockedMode=true
dotnet tests/Metallic.RC.NativeGateway.Tests/bin/Release/net10.0/Metallic.RC.NativeGateway.Tests.dll --self-test NEW-result.json
```

Independent security and CI gates are required before integration. No original game,
installed account data, public service, OS setting or privileged operation is
part of these checks; `nativeClientVerified` remains false.

The solution includes this test project. The existing read-only Windows/Linux
backend workflow adds its locked build and explicit self-test; its metadata-only
JSON report uses the existing backend artifact directory. Hosted CI has not yet
run for this candidate.
