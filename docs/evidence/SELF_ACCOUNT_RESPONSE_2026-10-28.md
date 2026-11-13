# Authored SelfAccount response profile

28 October 2026. This review candidate writes an explicit response format accepted by the bounded default-numeric reader prefix for retained Steam build **23191908**. It does not reconstruct the original client's serializer or native account construction. It adds no backend store mapping, identifiers, endpoint, authentication behavior or deployment change.

`NativeSelfAccountResponseProfile.Write` requires a `NativeSelfAccountResponseModel`. All **20 constructor arguments are required**; nullable values must be supplied explicitly. Model properties are immutable and incidental text output redacts their values. The writer does not infer a birthday, email, age, junior status or identity flags from other fields. Values remain the caller's explicit inputs.

| Canonical output key, in order | Explicit model value |
|---|---|
| Email | String or null |
| Phone | String or null |
| Birthday | UTC ticks or null |
| JuniorState | Int32 |
| ParentAccountId | Int32 or null |
| IsFakeJuniorBirthday | Boolean or null |
| AvailableUsernameChanges | Int32 |
| AccountId | Int32 |
| UserName | String or null |
| DisplayName | String or null |
| DisplayEmoji | String or null |
| ProfileImage | String or null |
| BannerImage | String or null |
| TreatAsJunior | Boolean |
| HasBirthday | Boolean |
| PersonalPronouns | Int32 |
| IdentityFlags | Int32 |
| createdAt | UTC ticks |
| IsMetaPlatformBlocked | Boolean |
| IsJunior | Boolean or null |

Every key is emitted, including explicit nulls, false and zero. The canonical keys/order come from the retained generated formatter constructor. Undefined enum Int32 values remain numeric. Birthday and createdAt use numeric ticks in the verified range **0 through 3,155,378,975,999,999,999**; the model names these inputs `BirthdayUtcTicks` and `CreatedAtUtcTicks`. No date string, epoch conversion, GUID-to-AccountId mapping, default account or eligibility rule is added.

## Admission and output bounds

The authored limits are **16,384 strict UTF-8 bytes per string** and **65,536 total output bytes**. String validation rejects unpaired UTF-16 surrogates before `Utf8JsonWriter` can replace them. Valid surrogate pairs are preserved. The output stream checks every write against the total limit, including JSON escaping expansion. Limits are candidate resource bounds, not recovered native service limits.

Static error codes distinguish `invalid_unicode`, `string_too_large`, `ticks_out_of_range` and `response_too_large`. Exceptions do not include caller values. The writer validates all strings and date inputs before serializing and returns a fresh byte array only after a complete bounded document is written.

## Executed writer-to-native-prefix comparison

New private v5 fixtures contain **512 fully specified synthetic models**. A compiled C# fixture executable constructs the typed model and calls the actual writer. It consumes no expected native results. The external comparison takes the writer's actual UTF-8 bytes, parses those bytes into the declared token-callback model, then feeds them into the retained native CPU-guest reader prefix. It compares all 20 captured presence/local values against an independent projection of the explicit model inputs. This is **not a C# reader roundtrip**.

All **512 responses matched with zero strict JSON-type/value/presence differences**, and every response had the exact canonical key order. The writer emitted 438–649 bytes per synthetic case. The actual native SelfAccount prefix guest counted **520,901 steps including initialization**; the enum numeric reader was visited **1,536 times**, and the nullable Birthday wrapper **512 times**. Native observations stop before the first SelfAccount allocation.

| Writer negative control | Native projection differences |
|---|---:|
| Drop IdentityFlags | 512 / 512 |
| Swap AccountId and AvailableUsernameChanges values | 512 / 512 |
| Convert date ticks to seconds | 453 / 512 |

The controls mutate the compiled writer's output only in the new fixture executable. They never alter production writer behavior or existing fixtures. The zero-date cases explain why the last control does not change every result.

The native comparison retains the previously declared models: lexical JSON/token callbacks, metadata and allocator/GC services, runtime generic/type/delegate/boxing services, default resolver selection and cross-guest date/enum interface delegation. The original lexical JSON parser, live resolver, full enum reflection initializer, original client serializer, account constructor/defaults/setters, client process and authentication were not executed. These results support the bounded default-numeric **prefix** profile only; they do not establish an original-client account-success response.

## Checks and preservation

The new executable passed **64 authored checks**: required constructor inputs, immutable properties, canonical order/null/false/zero handling, numeric dates and undefined enums, every string field's surrogate rejection, valid Unicode/escaping, UTF-8 byte admission, deterministic output, redaction, and exact **65,536-byte acceptance / 65,537-byte rejection**. Date range and null-model failures are included. These authored boundary checks are separate from the native observations.

The full solution built in Release with locked dependencies and zero warnings/errors. Existing readers were not modified. Recompiled comparisons still match immutable **v2: 1,292**, **v3: 1,768**, and **v4: 2,445** inputs with **zero differences**, and their unchanged authored checks pass **79 / 70 / 102** checks. The unchanged compatibility regression passed **570 cases / 4,996 checks** and the login profile passed **106 checks**. The compatibility fixture uses loopback only and stops its listener.

The new project is included in the solution and existing Windows/Linux CI matrix. CI runs authored checks without game inputs. Private synthetic models, full writer outputs, native observations, captured code and local analysis paths remain outside the repository. The adjacent aggregate JSON records sanitized hashes and totals. Source integration, full domain account behavior and native endpoint acceptance remain separate review gates.
