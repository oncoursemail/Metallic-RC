# Inactive native session storage

Board #366, branch `codex/rc-native-sessions-20261029`, based on credential
verification candidate `65f4b890970d165d10e98087aa250999c29e28da` (#93).

This implements durable session state for a later independently reviewed
native authentication adapter. It does not generate a JWT or other native
wire token, store a signing key, enable a route, or turn a parser result into
login success. `/recnet/auth/connect/token` still returns its existing 501.
No services, installed account databases or original clients were used.

## Activation and migration

`BackendOptions.EnableNativeSessions` is an embedding-only option, default
false. There is no CLI switch. It requires explicit identity mapping and a
pre-existing validated schema v2 database. Enabling it against v1 refuses
before the existing v1-to-v2 identity migration. The separate identity
activation step must finish first; no dummy identity is manufactured.

The session migration creates two tables and two indexes in one transaction,
validates their definitions/data, and commits schema v3. A collision rolls
back all new DDL and the version. Default-off v1/v2 databases receive no
native session schema. Opening a previously enabled v3 database with the
option off validates its state and preserves owned account/session APIs,
while every native session API stays disabled. Older binaries that only
accept v1/v2 cannot open v3; this is not a downgrade mechanism.

## Exact material and identity

The trusted embedding supplies `NativeSessionMaterial`: exact access and
refresh strings plus explicit absolute expiry times. These are untrusted
data to the store, not authorization claims. Admission requires 32–16,384
UTF-16 code units, strict valid UTF-8 within 16,384 bytes, no whitespace or
control characters, distinct material, and no previously recorded hash.
There is no trimming, normalization, claim parsing or token-format guess.
Only SHA-256 hashes enter SQLite; plaintext material has no public result
properties, default JSON representation or incidental text output.

`CreateNativeSession` first calls the reviewed password verifier, then
requires that verified GUID and the supplied native Int32 ID match the
durable identity map inside the transaction. It does not accept a Principal
constructed by the caller, create an account, allocate an identity, or mint
an unrelated owned session. `AuthenticateNativeSession` requires exact
stored material, the same account/native binding, current expiry, and an
unrevoked family. A decoded `sub` or other claim cannot authorize access.
Final hash comparisons use `CryptographicOperations.FixedTimeEquals`; the
SQLite lookup and whole operation are not claimed constant time.

Rotation requires the current refresh material and exact binding. It
atomically replaces access/refresh hashes and records both in permanent
per-generation history. The previous access immediately stops working. A
replayed consumed refresh for that binding revokes the entire family; that
revocation is committed before the static rejection is returned. An invalid
replacement rolls back without consuming a valid current refresh. Wrong-kind,
unknown, expired and revoked material is rejected. Explicit revocation uses
an authenticated current access token, never a caller-supplied Principal.

These are authored development limits, not recovered client policy:

| Limit | Bound |
| --- | --- |
| Access lifetime at issue/rotation | At most one hour |
| Family lifetime | At most 30 days from original creation |
| Rotations per family | 32 |
| Unrevoked, unexpired families per account | 8 |
| Durable families, including revoked/expired | 1,024 |
| Durable access/refresh hash rows | At most 67,584 |

There is no automatic history pruning. Exhaustion refuses new issuance or
rotation with a static limit error. Revocation can free active-family
capacity, but never the durable-family cap or recorded hash uniqueness.
A future maintenance/retention policy needs its own review.

## Storage and verification

The existing owner-only backend directory and lock protect state. Existing
consistent SQLite backups include native identities, sessions and rotation
history. The new tables require exact DDL (only CRLF/LF is normalized), no
triggers, valid foreign keys, canonical hashes, correct generation history,
bounded rows/times, and exact identity bindings. Validation deliberately
scans this small local store per operation; there is no throughput claim.
This is ordinary trusted local SQLite, not protection from a malicious
database administrator, rollback attacks or arbitrary process-memory reads.
Clearing caller strings or universally erasing managed memory is not claimed.

```powershell
dotnet build tests/Metallic.RC.NativeSessions.Tests/Metallic.RC.NativeSessions.Tests.csproj -c Release -p:RestoreLockedMode=true
dotnet tests/Metallic.RC.NativeSessions.Tests/bin/Release/net10.0/Metallic.RC.NativeSessions.Tests.dll --self-test NEW-result.json
```

The locked Release build passed with zero warnings/errors. All 122 new checks
use generated temporary databases and synthetic material only. They cover
default-off behavior, rejected v1 activation, atomic migration failure,
wrong credentials/identities, exact-token admission, redaction/hash-only
storage, rollback, concurrent refresh replay/revocation, expiry and original
family lifetime, restart, all capacity limits, malformed/triggered state,
and consistent backup/restore alongside the old owned session.
The unchanged identity and credential-verification executables also passed
111 and 134 checks respectively. Existing fixtures and tests were not edited.

Independent security review and source integration are still required.
The native token/key wire contract, account data, request eligibility,
signature verification, refresh endpoint and original-client acceptance
remain separate work. The existing sampled-body HMAC profile alone is not
full-body integrity and has not been integrated here.
