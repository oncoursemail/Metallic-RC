# SelfAccount reader-prefix candidate

28 October 2026. Authored compatibility component for Steam build **23191908**, retained as a review candidate. It has no endpoint, account constructor, writer, login-success path or deployment change.

The client-side request audit identifies a GET on the **Accounts** service (enum value 11), relative path **`account/me`**, with a generic `Task<SelfAccount>` response. GET was checked against this build's `HttpMethod.get_method` static field at offset 8, the native `get_Get` getter and the class initializer constructing that field from the literal `GET`. The path and generic response type were independently resolved from captured metadata slots. These are static client observations; they do not establish server-side authentication or a working native request.

The initial account-fetch unwind record contains 256 bytes and falls through. Its adjacent 451-byte record carries `UNW_FLAG_CHAININFO` linking back to that first record. The combined 707-byte span independently decodes to 152 valid instructions. An earlier interpretation of Ghidra's 701-byte body as a 445-byte overshoot was incorrect: 256 bytes was only the first compiler fragment. Ghidra omits the final six-byte null-error call/trap path and still reports unresolved flow. This correction does not make the decompilation complete source.

## Verified prefix behavior

`SelfAccountPrefixContract.Read` preserves only properties present in the supported input. It keeps the generated formatter's 20 canonical names and 57 exact raw aliases, including lowercase **`createdAt`** as the canonical writer spelling. It does not add blanket case folding or escaped-name normalization.

| Fields | Prefix profile |
|---|---|
| Email, Phone, UserName, DisplayName, DisplayEmoji, ProfileImage, BannerImage | String or explicit null |
| AvailableUsernameChanges, AccountId | Int32 |
| TreatAsJunior, HasBirthday, IsMetaPlatformBlocked | Boolean |
| Birthday, JuniorState, ParentAccountId, IsFakeJuniorBirthday, PersonalPronouns, IdentityFlags, createdAt, IsJunior | Outside this profile, including null |

The generated reader's jump-table cases select the observed string, Int32, Boolean and nested-reader helpers. The supported primitive results were collected from its actual presence bytes and stack/register locals after property parsing. The probe stops before the first account allocation. No account defaults, constructors, setters, nested formatter semantics or authentication are modeled into successful account objects.

- Root null and an empty object have distinct result statuses; both carry null fields.
- A nonempty object containing only unknown properties completes with an empty presence map.
- Duplicate recognized aliases use the last value. Absent properties remain absent; explicit string null remains present.
- A wrong primitive type returns `known_value_type_error`. A recognized nested field returns `nested_value_outside_profile` before any value is admitted, even for null.
- The read-only result map redacts values from incidental `ToString` output.

The authored admission limits are 65,536 input bytes, JSON depth 32 and 512 properties. These are candidate bounds, not recovered native protocol limits. Strict UTF-8 admission applies to the entire byte input, including unknown properties; invalid bytes return the static `invalid_utf8` exception code. Invalid escaped surrogate sequences in recognized string values become the static `invalid_string` exception code rather than leaking an uncaught string-decoder exception. This lexical hardening has authored checks, not a native lexical-equivalence claim. Successful prefix parsing checks the complete object and trailing input. Failure/outside-profile statuses stop early and are not complete-document validity or account-success claims.

## Executed comparison

A new fixture executable reads `{id, inputJson}` and emits `{id, status, fields}`. It never consults the fixture's expected observations. The independent comparison checks status, field presence and exact JSON scalar types/values against **1,292 synthetic native-prefix observations**: **zero mismatches**.

The captured formatter constructor and reader prefix ran in an isolated Unicorn CPU guest: 1,014 constructor steps and 280,347 fixture steps. Metadata, allocation, GC barriers, key encoding, alias lookup and JSON token callbacks are explicit modeled boundaries. The original lexical JSON reader was not executed. The original game was never started as a process.

| Compiled comparison | Mismatches among 1,292 cases |
|---|---:|
| Actual component | 0 |
| Negative control: case folding | 20 |
| Negative control: first duplicate wins | 224 |
| Negative control: empty object treated as a profile | 1 |
| Negative control: swapped integer fields | 232 |

The separate `--self-test` run passed **79 authored checks** for primitive boundaries, all eight outside-profile fields, exact aliases, duplicate/null/presence behavior, malformed input, limits and result handling. Eleven cases added after independent review cover invalid raw UTF-8 in recognized values, unknown values and property names, overlong/truncated/surrogate byte encodings, lone escaped surrogates and valid Unicode pairs. These checks are separate from native observations. The first self-test exposed empty-input exception classification; the component now checks empty input explicitly. That failed candidate receipt remains private. The original 1,292 native-prefix fixtures remain unchanged and still compare with zero differences after lexical hardening.

The full solution built in Release with locked restore and zero warnings/errors. The unchanged discovery/loopback regression passed 570 cases and 4,996 checks, and the unchanged login-profile regression passed 106 checks. Those authored-service checks do not establish original-client acceptance.

The synthetic native observations, captured instructions, dumps, private analysis paths and per-case comparison outputs are not committed. The adjacent aggregate JSON retains hashes and measured totals only. CI runs authored checks without private game inputs:

```text
dotnet build tests/Metallic.RC.SelfAccountPrefix.Tests/Metallic.RC.SelfAccountPrefix.Tests.csproj -c Release -p:RestoreLockedMode=true
dotnet tests/Metallic.RC.SelfAccountPrefix.Tests/bin/Release/net10.0/Metallic.RC.SelfAccountPrefix.Tests.dll --self-test NEW-result.json
```

The remaining gates are independently verified nested formatter semantics, account construction/defaults/setters, native authentication and an observed original-client connection. This component does not enable native routes or change backend capability claims.
