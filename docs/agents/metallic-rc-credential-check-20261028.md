# RC development-store credential checking without session issuance

Board #323; branch `codex/rc-credential-check-20261028`, based on identity-mapping
candidate `9700182` / PR #92.

This is maintenance of the existing isolated RC development store, not a new
production sign-up or password authority. The project's established direction
is **one Metallic account system for the launcher, STAR and RC**, documented in
the workspace's `metallic-accounts-20261026.md` / central accounts PR #693.
Actual RC sign-in should integrate that existing Metallic accounts authority
after controlled service, token-validation and profile/subject-mapping design.
It must preserve the authority's MFA/passkey and optional-provider decisions,
rather than duplicate users' passwords into this development store. This patch
adds no registration endpoint or launcher account flow.

`BackendStore.VerifyCredentials(LoginRequest)` is an internal helper that returns
the existing owned `AccountView`. It contains the previous `Login` validation,
account lookup, dummy-hash password verification for unknown accounts, and
`invalid_credentials` rejection without changing that logic. `Login` now calls
the helper, then performs its unchanged bearer generation and session transaction.

The helper does not mint and discard an owned bearer to test a password. It
does not insert/prune sessions, allocate a native identity, update a profile or
password hash, or expose a new route. The existing hasher configuration and
acceptance of `SuccessRehashNeeded` without upgrading the stored hash are
preserved. This extraction does not claim to improve password timing or introduce
rehash policy. Caller-supplied input and password strings remain managed memory;
credential erasure is not claimed.

The helper remains assembly-internal. The new console checks bind its actual
method with reflection; no public API, friend assembly or injected production
hasher is added for tests. Tests use only newly generated temporary databases and
synthetic credential values created in memory. They cover:

- Correct, wrong, unknown, malformed and case-normalized inputs; unchanged static
  error codes; no helper console output on success or failure.
- No helper session writes, including preservation of preexisting expired rows;
  no native mapping rows/counter allocation when mapping is opted in; unchanged
  v1/v2 selection and profile/password hashes.
- Returned `AccountView` formatting and JSON contain profile metadata without a
  password or hash. This does **not** claim that all preexisting credential record
  formatters are fixed: the independently reviewed PR #91 companion must be
  preserved when integrating its separate `ToString` redaction changes.
- Existing owned `Login` output: distinct 64-hex bearers, one-hour expiry,
  SHA-256-only storage, authentication, sixteen-session cap, expiry cleanup,
  logout/revocation and replacement of a freed session slot.
- A generated lower-iteration hash genuinely returning `SuccessRehashNeeded`,
  accepted by both helper and `Login` with the original no-rehash-write behavior.

New project/lock, solution registration and additive Windows/Linux CI steps are
included. Existing tests and fixtures are unchanged. The native endpoint still
returns its existing 501. Production RC sign-in and its native-session lifecycle
require the central-authority integration described above; this extraction is
not that sign-in path or verified Steam-independent native login. It does not
change Steam, client protection, the launcher, installed data or deployment.

```powershell
dotnet build tests/Metallic.RC.CredentialCheck.Tests/Metallic.RC.CredentialCheck.Tests.csproj -c Release -p:RestoreLockedMode=true
dotnet tests/Metallic.RC.CredentialCheck.Tests/bin/Release/net10.0/Metallic.RC.CredentialCheck.Tests.dll --self-test NEW-result.json
```

Local Windows SDK `10.0.402` locked Release build passed with zero warnings/errors,
and all 134 synthetic assertions passed. Independent review reran those final
134 checks and compared the actual source to the base commit: both the extracted
verification prefix and remaining session suffix are unchanged. Source/workflow
review and `git diff --check` passed. Hosted CI, native execution, account sign-in
and deployment are not claimed here.
