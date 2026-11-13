# Explicit native account profile storage

Board #371, branch `codex/rc-native-profiles-20261029`, based on reviewed native
session commit `f3b79789f8bf5d8afcca4a193c635da13751a012` (#104).

This adds inactive storage for the existing 20-field SelfAccount response
model. It reuses `NativeSelfAccountResponseProfile.Write`; it does not add a
native formatter, account factory, JWT, signing key, successful login or game
route. The exact intended native target remains GET
`/recnet/accounts/account/me`; this change does not enable it.

## Explicit activation and authority

`BackendOptions.EnableNativeAccountProfiles` is an embedding-only option,
default false. It requires identity mapping and an already initialized,
validated v3 database. The precondition is checked before earlier migrations,
so rejected v1/v2 activation cannot silently initialize identity/session state.
The explicit v3-to-v4 migration creates its table and unique index in one
transaction, validates them, and advances the version only on success.
Default-off v1/v2/v3 databases gain no profile table. Previously enabled v4
databases validate all native state even when opened off, and retain owned
account/session APIs. Older binaries do not gain v4 downgrade support.

Writes require the real local administrator key through the existing IsAdmin
check. They do not accept a caller-created Principal or a model's assertion
of authority. All 20 immutable model arguments must be supplied explicitly,
including every null, zero and false value. The account GUID/native Int32 pair
must match the durable identity map; AccountId must equal that Int32, and
UserName/DisplayName must equal the actual current owned account names.
The existing writer enforces string/date/output bounds before persistence.

The remaining values are explicit owner-entered data. Nothing derives a
birthday, junior/adult status, parental relationship, creation date, pronouns,
privilege flags, image, contact data or username-change allowance. Undefined
Int32 enum values admitted by the existing writer stay data, not permission.
There is no JSON administration endpoint; a future adapter must establish
explicit input presence and owner authorization before constructing a model.

Admin reads require the same real key. Session reads use the existing exact
native-session authenticator and identity binding inside the same transaction
as profile lookup. Owned sessions, refresh tokens, forged account IDs and
caller-created Principal objects cannot authorize native profile reads.

## Revisions and stale names

One row per account stores the canonical response BLOB, a positive profile
revision and the owned account revision captured at write. Revision zero is
the explicit create expectation; updates must match the current profile
revision. Competing updates cannot silently overwrite one another, and
Int64 revision exhaustion fails rather than wrapping.

An owned display-name/profile revision change preserves the stored native
profile verbatim. Reads return `native_profile_stale` until an administrator
explicitly rewrites all inputs against the current owned account. Startup
preserves structurally valid stale data and keeps owned APIs usable. This
is a conservative snapshot policy, not an inferred native account rule or
automatic synchronization of age/permission fields.

## Bounds, privacy and consistency

The authored cap is 1,024 profiles, each at most 65,536 bytes. One account and
one native identity can own only one row. Schema/type/count/length metadata is
checked before a BLOB is allocated or decoded. Startup validates every payload;
normal operations validate schema/metadata and decode only the selected row.
The selected row is read inside a transaction with SQL size/type admission.

Storage validation requires strict UTF-8, exactly all 20 known properties,
their exact primitive/null types, and byte equality with the existing writer's
output. Duplicate, missing, unknown, reordered/noncanonical or malformed
payloads are refused. This is an authored canonical storage contract, not a
claim about arbitrary native parser inputs. Unexpected triggers, wrong schema,
foreign-key/binding inconsistencies and invalid revision metadata are rejected.

The existing owner-only SQLite directory, process lock and consistent backup
mechanism protect and preserve this state. Payload access is explicit and
returns copies. Default JSON/text diagnostics expose revision metadata or
static errors, never the profile BLOB, contact values, tokens or admin key.
The administrator credential remains separate from the database-only backup.
This is trusted local storage, not protection from a malicious local database
administrator, history rollback, process-memory inspection or stolen keys.
No throughput or original-client readiness claim follows from these checks.

## Reproducible checks

```powershell
dotnet build tests/Metallic.RC.NativeAccountProfiles.Tests/Metallic.RC.NativeAccountProfiles.Tests.csproj -c Release -p:RestoreLockedMode=true
dotnet tests/Metallic.RC.NativeAccountProfiles.Tests/bin/Release/net10.0/Metallic.RC.NativeAccountProfiles.Tests.dll --self-test NEW-result.json
```

Checks use newly generated temporary databases, synthetic explicit models,
and the actual store/writer methods. They cover activation/version rollback,
admin/identity/session authorization, null/false/zero/Unicode data, immutable
views, revision races, stale-name preservation and explicit repair, malformed
payloads, preallocation bounds, selected-row decoding, triggers, row capacity,
revision exhaustion, restart and consistent backup/restore.

The locked Release build passed with zero warnings/errors, and all 143 new
checks passed. The unchanged native-session suite passed 122 checks and the
unchanged response-writer suite passed 64 checks. These local source checks
are separate from hosted CI or native-client acceptance.

The native-session source/tests and existing response writer remain unchanged.
No installed account database, service, native process, original game or
privileged operation was used. Source and workflow integration still require
independent review before any commit or later endpoint activation.
