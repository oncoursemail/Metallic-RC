# Default-off native identity mapping

Board item #322, branch `codex/rc-native-identity-20261028`, based on the explicit
account-response candidate `bcbf457`. This adds server-local positive Int32 IDs
for existing Metallic-owned GUID accounts. The IDs have no connection to
production account IDs and grant no authentication or account privileges.

## Activation and migration

`BackendOptions.EnableNativeIdentityMapping` is an embedding-only `init` option
whose default is `false`. `BackendOptions.Parse`, command-line flags, native
routes, capabilities, credential issuance and installed services are unchanged.
No installed database was opened or migrated during this work.

- Default-off new and existing v1 databases remain schema v1; no mapping table
  or mapping allocation is created.
- Explicit opt-in upgrades v1 to v2 in one SQLite transaction. It creates the
  mapping table and unique account index, validates them, then commits the new
  version. A failure on the second DDL statement rolls back the first statement
  and leaves the original schema version, index and owned records intact.
- Previously opted-in v2 databases still support the owned account/session APIs
  when the option is off. Mapping APIs remain disabled. Invalid v2 identity
  schema/data is rejected even when the option is off.
- Unknown database versions are rejected. This patch does not downgrade a v2
  database for older server binaries that only accept v1.

`GetOrCreateNativeAccountId` requires an existing canonical lowercase, nonempty
GUID in `N` format. The separate mapping table uses an `AUTOINCREMENT` primary
key constrained to `1..2147483647`, a unique account index and a foreign key to
`accounts(id)`. `AccountIdForNative` resolves the exact reverse binding. Neither
API derives an ID from GUID bits or a hash, changes a GUID, or creates an account.

The existing store owner lock, its operation lock and SQLite transactions
serialize allocation. Existing bindings survive restart and backup/restore.
Exhaustion refuses a new binding while existing bindings remain readable; IDs
do not wrap, and `AUTOINCREMENT` does not reuse a deleted high ID.

## Validation boundaries and cost

The authored table/index DDL must match exactly after **only CRLF-to-LF line
ending normalization**. New table DDL is also created with LF endings. This
permits a database to move between builds whose source checkouts use different
line endings, without accepting altered constraints or an ordinary replacement
index. Unexpected triggers on the mapping table are rejected. A discovered
AFTER INSERT trigger could otherwise redirect a valid allocation from requested
account A to an existing account B. Allocation additionally checks its returned
ID still binds the requested GUID before committing.

Validation checks foreign keys, canonical stored GUIDs, positive Int32 IDs,
sequence type/range/uniqueness, and that the sequence is not behind allocated
IDs. It runs at v2 startup and within each mapping operation's transaction.
This deliberately scans the mapping table and is **O(N) per lookup/allocation**;
the candidate targets small local maps and makes no large-scale latency claim.

The database remains trusted local state protected by the existing owned data
directory and process lock. This is not a tamper-proof identity ledger: an
external database administrator can coherently rewrite valid rows/counters or
replace an entire database. The public API exposes no mapping delete or rebind.
Malformed-state regression coverage is not proof against arbitrary disk faults
or malicious database rewrites.

## Verification

The new `Metallic.RC.NativeIdentity.Tests` console project uses only newly
generated temporary databases and synthetic account values. Its fixture copier
takes the backend owner lock, checkpoints WAL with a busy-result check, verifies
that WAL is drained, and then copies the database. Tests explicitly refuse an
active store and preserve committed WAL state. This helper is not a general live
database backup API; the production store's existing SQLite backup API is also
tested for mapping persistence.

The standalone checks cover default-off behavior, atomic migration rollback,
foreign-key and uniqueness constraints, canonical GUID rejection, concurrent
same/different-account allocation, reopen, v2 default-off owned sessions,
Int32 exhaustion, nonreuse, unexpected-trigger redirection, malformed schema and
sequence states, unchanged database bytes after failed validation, stored CRLF
and LF schema SQL, and backup restore. Recursive fixture cleanup first verifies
the full path equals the generated name under the canonical temporary directory,
the expected prefix/GUID suffix and that the root is not a reparse point.

```powershell
dotnet build tests/Metallic.RC.NativeIdentity.Tests/Metallic.RC.NativeIdentity.Tests.csproj -c Release -p:RestoreLockedMode=true
dotnet tests/Metallic.RC.NativeIdentity.Tests/bin/Release/net10.0/Metallic.RC.NativeIdentity.Tests.dll --self-test NEW-result.json
```

The new project/lock is registered in the solution and the existing Windows/Linux
CI matrix with additive build/check steps. Local Windows SDK `10.0.402` locked
Release build passed with zero warnings/errors, and all **111 assertions** passed.
Independent review reran the locked build, all 111 checks, and the reviewer's
original actual-API A-to-B trigger probe: it now rejects before allocation.
Source/workflow review and `git diff --check` passed. Hosted Linux execution is
not claimed. Existing tests and fixtures are unchanged. Integration, installed
database migration and native login acceptance remain separate steps.

The parent independently built the full solution in locked Release mode with
zero warnings/errors, reran all111 mapping assertions, and ran the unchanged
backend transport executable:92 checks and83 isolated HTTP requests passed.

## Next authentication boundary

The owned `BackendStore.Login` couples password checking to creation of a
one-hour opaque 64-hex session. It must not be called simply to test a native
password and discard that owned session. A later reviewed slice should extract
the credential verification helper, preserve existing owned-session behavior,
and introduce native session state separately. Existing `Authenticate` accepts
only the owned token format; no refresh-token storage exists today.

The current owned profile contains GUID, username, display name and revision.
This mapping supplies the additional native Int32 identity. It does not supply
birthdays, junior/parental state, role privileges, original creation time,
identity flags, pronouns or other missing account data. The explicit native
response writer requires callers to choose every value; missing data must not
be laundered into invented adult/permission defaults or claimed login success.
Native token, refresh and signing-key policies still require their separate
reviewed consumer evidence and session-lifecycle design.
