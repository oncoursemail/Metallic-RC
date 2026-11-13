# Default-resolver nullable prefix candidate

28 October 2026. This extends the authored SelfAccount reader prefix for retained Steam build **23191908** through a separate, explicit API:

`SelfAccountPrefixContract.ReadWithDefaultNullablePrimitives` admits **ParentAccountId** as Int32 or null and **IsFakeJuniorBirthday / IsJunior** as Boolean or null. It retains the original 12 primitive fields. `Read` keeps its existing 12-field behavior and still stops on all eight nested fields. Neither API constructs an account, supplies defaults, serializes an account, enables a route or authenticates a client.

## Resolver and reader evidence

The captured serializer's default-resolver getter (metadata method 347097) reads its configured resolver first. Its null fallback selects `LZBFBYNGFNJ`'s field zero, initialized from `Internal.TUBRHDQOFUC`. That resolver's `ULFZXYQUZCO` cache walks the resolver array initialized by method 347197. The captured order is built-in `ISWXCMNVVSF`, `IUXFSWELTAD`, `EDKCNZAQDUU`, then `UTGCZKGOKGM`. The exact nullable Int32 cache specialization independently confirms iteration and first non-null selection.

The built-in dictionary initializer (347164) explicitly maps `Nullable<int>` to `PHFLTGVCCAH` and `Nullable<bool>` to `IZJHBECBGLZ`. Their native readers, 347800 and 347924, first call the null-token reader. Null returns a zero nullable representation; a non-null value calls the Int32 or Boolean reader, followed by a correctly typed `Nullable<T>` constructor. Generic metadata slots distinguish these constructors from unrelated methods sharing their machine-code addresses. Both constructor leaves have three instructions through their first return, storing presence and the scalar value.

This is an **explicit default-resolver profile**. Application-supplied or subsequently configured custom resolvers remain outside the claim. Static default selection does not establish which resolver an unobserved live request used. Birthday, JuniorState, PersonalPronouns, IdentityFlags and createdAt still return `nested_value_outside_profile`, including null. Date and enum defaults are customizable and are not inferred from CLR types or a public library version.

## Presence and admission behavior

- All 57 observed raw aliases remain exact. No case folding or escaped property-name expansion is added.
- Omitted properties remain absent. Explicit nullable null stays present; false and zero stay distinct from null. The last recognized alias wins.
- Wrong non-null types return `known_value_type_error`. Integers are signed Int32; Boolean numbers and strings are rejected.
- Root null, empty-object null, unknown-property skipping, strict UTF-8 admission and authored byte/depth/property limits are shared with the existing reader.
- Failure and outside-profile statuses stop early. They do not establish complete-document validity or successful account construction.

## Executed checks

A new private v3 run executed the captured SelfAccount formatter constructor, property dispatch, presence/local stores, nullable readers and both typed nullable constructor leaves in an isolated Unicorn CPU guest. It stopped before the first SelfAccount allocation. **1,768 synthetic inputs** ran through **537,141 total guest steps, including formatter/bootstrap initialization**. Nullable Int32/Boolean reader entries were visited 1,036/2,130 times; their constructor entries were visited 790/1,297 times. JSON token callbacks, metadata resolution, allocation/GC barriers, default-resolver selection and interface delegation remain declared models. The original lexical reader, live resolver execution, account construction and original game process were not executed.

An independent authored reference and the compiled C# component each matched all **1,768** observed statuses and presence/value maps with **zero strict JSON-type mismatches**. The C# fixture executable reads only input identity/document and never reads native expected results. Comparison runs outside that executable.

| Compiled negative control | Detected differences |
|---|---:|
| Reject explicit nullable null | 312 |
| Accept numeric values for nullable Boolean | 60 |
| Omit explicit nullable null | 219 |
| First duplicate wins | 506 |

The Python reference's controls have a broader mutation scope and separate measured totals; they are not substituted for these compiled-control results.

The new executable passed **70 authored checks**, including signed boundaries, three-field null presence, false/zero distinctions, exact and escaped aliases, duplicate order, remaining unsupported fields, unknown nested values, malformed input and read-only results. The unchanged original executable passed **79 checks**, and `Read` still matched all **1,292 immutable v2 inputs** with zero differences. The full solution built in Release with locked dependencies and zero warnings/errors. Unchanged compatibility checks passed 570 cases / 4,996 checks and the login profile passed 106 checks. The compatibility fixture used loopback only and stopped its listener; these checks establish no original-client acceptance.

The new test project is included in the solution and the existing Windows/Linux backend CI matrix. CI runs authored synthetic checks only. Private native pages, per-case observations, synthetic evidence fixtures and local analysis paths remain outside the repository. The adjacent aggregate JSON records their hashes and totals. This candidate has not been deployed and changes no native capability claim.
