# Explicit numeric SelfAccount prefix candidate

28 October 2026. Review candidate for the retained Steam build **23191908**. A third API, `SelfAccountPrefixContract.ReadWithDefaultNumericScalars`, adds numeric enum values and numeric DateTime ticks to the earlier profiles. `Read` retains its 12-field behavior; `ReadWithDefaultNullablePrimitives` retains its 15-field behavior. This candidate does not construct accounts, supply defaults, write responses, enable routes, authenticate clients or deploy services.

## Explicit representation and boundaries

| Additional fields | Numeric profile |
|---|---|
| JuniorState, PersonalPronouns, IdentityFlags | Signed Int32, including undefined enum values; strings remain outside the profile |
| Birthday | Explicit null or numeric UTC ticks |
| createdAt | Numeric UTC ticks; null remains outside the profile |

Numeric dates produce a get-only `SelfAccountDateTimePrefixValue` containing `Ticks` and `Kind = DateTimeKind.Utc`. This is diagnostic reader-prefix data. It is not a recovered wire-date string, an account property assignment or a writer contract. Its incidental text representation redacts values. The supported native tick range is **0 through 3,155,378,975,999,999,999**. Int64 lexical admission errors produce `known_value_type_error`; a numeric value outside the native DateTime constructor's range produces `known_value_range_error`.

All nonnumeric date inputs remain `nested_value_outside_profile`, except the verified nullable Birthday null path. This intentionally leaves date strings, Boolean/object/array date values and non-nullable DateTime null unresolved. Enum strings also stop outside the profile; enum null, Boolean and compound values follow the observed error branch. Existing exact aliases, present-only fields, last-alias-wins behavior, shared admission limits and early-stop semantics are unchanged.

## Default enum binding and numeric path

The exact SelfAccount case metadata requests `IJsonFormatter<T>` separately for JuniorState, PersonalPronouns and IdentityFlags. Each type is an enum whose recovered `value__` field is Int32. The previously traced default resolver array tries the built-in provider first and `IUXFSWELTAD` second; the latter's default instance is `Internal.AENVFJVLGEY`. Its generic cache checks `System.Type.get_IsEnum`: native virtual slot 69 matches the typed method metadata. The enum branch constructs `JNNNEYSAILQ<T>(true)`.

The generic function registry resolves the captured Int32 shared implementation of that formatter. Its numeric token branch calls the stored underlying-value delegate. The native helper 347713 selects boxed reader **347728**, which calls **ReadInt32** and boxes with the same runtime class global used by the helper's selected type-comparison branch. That binding is checked against the reader instructions, rather than inferred from a conventional runtime structure layout. The formatter constructor 347733 copies the selected delegate; formatter 347735 calls closure 347745, then reader 347728, then unboxes the result. Undefined signed Int32 values are preserved, including negatives and Int32 extremes.

The generic enum static initializer's delegate-store path is statically traced. Its reflection/name-table initialization was **not fully executed**. Runtime generic context, type handles, underlying-type reflection, equality, delegate construction and boxing/unboxing are declared probe models. The profile does not claim a live request used the default resolver, and it makes no claim about custom resolvers or enum name parsing. An independent read of this build's compressed metadata constants and attribute ranges established the enum declarations; those names and declarations do not become additional accepted wire strings.

## Numeric DateTime and nullable execution

Exact JsonToken metadata confirms Number = 5 and String = 6. DateTime reader 347656's numeric branch calls ReadInt64, the native ticks constructor, `get_Kind`, `SpecifyKind` and the ticks/kind constructor. The executed valid path preserves ticks and produces UTC DateTimeData. The upper-bound check comes from the native constructor branch. The nullable DateTime specialization, method 347760, calls the null-token reader first; null returns a zero nullable representation, otherwise it calls the DateTime formatter and the typed Nullable<DateTime> constructor.

An isolated numeric enum run covered 274 synthetic inputs: 265 signed Int32 results, eight type/range rejections and one explicitly unsupported string. The selected delegate helper, formatter constructor, formatter numeric branch, closure and boxed reader instructions executed. An isolated DateTime run covered 271 inputs: 260 valid UTC-tick results, four native tick-range rejections, two modeled Int64 lexical rejections and five inputs stopped at the unverified sequence-reader boundary. That last group includes strings and invalid token kinds; it is not claimed as a native rejection result. A separate nullable DateTime run executed nine inputs, including a zero nullable result and three typed nullable constructor paths.

## v4 prefix comparison

New private v4 fixtures preserve all earlier inputs and add numeric date/enum cases, boundaries, aliases and repeated properties. **2,445 inputs** produced **zero strict JSON-type mismatches** against both an independent authored reference and the compiled C# component. Native prefix locals were read from the actual generated SelfAccount reader: Birthday's XMM6 nullable flags/data, createdAt's DateTimeData register, and the three enum Int32 locals/presence bytes. Execution stops before the first SelfAccount allocation.

The SelfAccount prefix guest counted **770,521 total steps including initialization**. Nullable primitive instructions execute in that guest. Date/enum reader instructions execute in separate bounded CPU guests and return their observed data through an explicitly modeled cross-guest interface delegation. JSON token callbacks, metadata/allocation/GC services and the runtime services listed above remain models. The original lexical JSON reader, live generic resolver, full static reflection initializer, domain constructor, original client process and native authentication were not executed. This is a bounded compatibility comparison, not a fully verified native account parser.

| Compiled negative control | Detected differences among 2,445 inputs |
|---|---:|
| Treat ticks as Unix seconds | 399 |
| Report unspecified kind | 399 |
| Restrict enums to declared constants | 459 |
| Omit explicit Birthday null | 88 |
| First duplicate wins | 827 |

The executable reads only fixture identity/input document; the external comparator reads expected native observations. Its negative controls alter diagnostic results or duplicate handling solely to check comparison sensitivity. The private v2 and v3 fixtures were not modified. The original readers still match **1,292 v2 / 1,768 v3 inputs with zero differences**, and their unchanged authored checks still pass **79 / 70** checks. The new executable passes **102 authored checks**. The whole solution builds in Release with locked dependencies and zero warnings/errors; unchanged compatibility checks pass **570 cases / 4,996 checks**, and the login profile passes **106 checks**. The compatibility fixture uses loopback only and stops its listener.

CI adds the new authored test project to the existing Windows/Linux matrix. It uses no private native fixtures. Native pages, private per-case observations and local analysis paths remain outside source control; the adjacent summary contains sanitized hashes and measured totals only. No source integration, native connection, deployment, release or account-success behavior is implied by these results.
