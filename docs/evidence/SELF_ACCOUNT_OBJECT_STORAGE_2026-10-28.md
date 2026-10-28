# Captured SelfAccount construction and storage observations

28 October 2026. This separate **v6** experiment extends the earlier reader-prefix and authored-response comparisons through captured constructor, assignment, protected-value conversion and getter instructions in bounded CPU guests. It changes no writer, application route or deployment. It does not establish original-client, complete domain-account, active-detector or authentication behavior.

## Constructed-object comparison

The input is the existing **512 fully specified synthetic models and their actual compiled writer UTF-8 outputs**. The writer outputs and earlier v2–v5 fixtures remain unchanged. The generated reader now proceeds beyond its prefix: it allocates a baseline SelfAccount and a destination SelfAccount, calls the captured constructors, assigns the 20 properties and returns the destination object. The returned pointer is checked against the second allocation's captured SelfAccount type identity, rather than accepting any allocated object.

The parameterless SelfAccount constructor is a tail call into the Account constructor. That body obtains a dependency-injection container, resolves an OnlineRestrictions service and stores the service reference. The guest supplies explicit synthetic container/service handles. This service model is a boundary; the real application's startup, dependency registration and service behavior are not reconstructed.

Across all **512 inputs**, the native assignment trace followed the observed canonical 20-field order. Values read from the final constructed object matched the explicit model projection with **zero strict type/value/presence differences**. This comparison uses final object storage, not the earlier parser locals or a C# reader roundtrip.

| Final-object observation | Executions |
|---|---:|
| Actual captured property-getter bodies: 18 per object | 9,216 |
| Actual generated formatter inline field reads: 2 per object | 1,024 |
| SelfAccount allocations: baseline and destination | 1,024 |
| Reader/construction guest steps | 2,023,874 |

A separate completion audit replayed the same 512 inputs without changing the frozen probes. It confirmed every property call returned normally with successful status and the expected return PC. Each inline read finished at its exact next instruction. The completion audit again found zero value differences.

An independent reviewer replayed all 512 inputs and confirmed those normal
returns, instruction endpoints and zero differences. The reviewer also checked
the actual returned DateTime kind bits for all 341 non-null Birthday values:
each was UTC. This extra check reads the getter output buffer rather than
relying on the projection's UTC label. The 64 controls and 23 sparse observations
were independently replayed too, with protected source and inputs unchanged.

## Two unavailable getter bodies

Captured **get_JuniorState (146330)** and **get_CreatedAt (146301)** entries jump outside the admitted PE image. They were not followed, restored or replaced with invented getter implementations. For those two values, the experiment executes the generated formatter's original inline backing-field read instructions against the final destination object. These are explicitly distinct from property-getter executions. The other 18 property-getter bodies execute directly.

The wire UserName assignment binds the raw-username accessor. The experiment does not establish rendered/platform-filtered username behavior, asynchronous post-deserialization hooks or other higher-level account behavior.

## Protected storage and runtime models

The captured ObscuredString, ObscuredInt and ObscuredBool conversion/encoding/decoding instructions execute, along with their captured static initializers. Their storage is not replaced with plaintext maps. Actual getter/decode instructions recover the supplied values from the constructed object.

Additional checks observed **2,393 nonempty string payloads stored as native cipher bytes differing from their UTF-16 inputs**, **907 protected integer stores differing from their plain values**, and **1,024 protected Boolean stores distinct from plain zero/one**. Empty and null strings are counted separately. These checks supplement the end-to-end value comparison; they are not a security assessment of the encoding.

Declared models remain:

- Zeroed typed allocation, metadata resolution and GC write barriers.
- Synthetic dependency-injection container and OnlineRestrictions handles.
- Ordinary UTF-16 string/character-array/byte-array primitives and Buffer.BlockCopy.
- A bounded, seeded Next(min, max) random callback used by the native Boolean constructor.
- Fresh isolated class-static detector-instance storage with no live instance. The captured detector-state predicate reads that guest state; active-detector paths are unverified.
- The earlier token/default-resolver models and cross-guest numeric date/enum reader delegation.

No live protection or configuration is changed. The native protected conversion bodies run in this isolated state; the results do not establish behavior with an active detector or other application runtime state. Ordinary-runtime callbacks are bounded, and each guest call has a 30,000-instruction limit. These are probe bounds, not recovered client protocol limits.

## Controls and sparse inputs

A negative control swaps the two actual numeric getter bindings for PersonalPronouns and IdentityFlags. It produces **58 differences among 64 inputs**; the six equal-valued cases remain equal. This validates that the comparison observes the final object and its getter mapping.

There are **23 separate sparse/root observations**: root null, an empty object, an unknown-only object, and 20 cases omitting one field. Root null and the empty object return null without constructing an account. The 21 nonempty cases construct objects under the declared models; supplied fields remain intact.

The unknown-only case observes null/zero/false values under the zeroed allocator and captured getter behavior. Its createdAt field is zero ticks with **Unspecified** kind, whereas explicit numeric date input zero is converted by the formatter to **Utc** kind. These are conditional native-guest observations. They do not become writer defaults, a backend account factory, invented birthday/age data or an eligibility policy. No production behavior is enabled from the sparse results.

## Retained evidence

The aggregate JSON beside this document records source, probe, writer-output and receipt hashes; measured totals; the two unavailable getter-entry hashes; and the model limitations. Native code bodies, per-case models, object storage, private paths and full observations remain private. The original failed absence comparator and its receipts were retained; its missing-key comparison bug was corrected and the bounded experiment was rerun. This failure did not come from native constructor or getter execution.

The response writer and its harness remain unchanged. Backend native routes remain unenabled; no original client, service, broadcast, account or authentication flow was started.
