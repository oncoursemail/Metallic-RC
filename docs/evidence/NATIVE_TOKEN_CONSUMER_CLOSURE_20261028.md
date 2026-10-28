# Bounded native token-consumer evidence

28 October 2026. Private CPU-guest and static evidence for retained build **23191908**. This note adds evidence to the pure signing candidate; it changes no production code, token issuance, route, account, profile, or permission. No real token, credential store, service, Frida session, or original client process was used.

## Positive paths tested

Four hash-verified native bodies executed in fresh bounded Unicorn 2.1.4 guests: credential setter 149439, getter 149440, subject reader 149438, and payload helper 152769. Six synthetic positive cases counted **38,300 native instructions** and **962 modeled callback entries**, separately. Raw token bytes and decoded signing-key bytes were recovered unchanged by the native getter. The scenarios were:

| Scenario | Bounded observation |
|---|---|
| Decimal string subject with no role/privilege claims | Setter/getter and subject reader complete |
| Two segments and a one-byte decoded key | These same paths complete; no third segment is read |
| Maximum signed Int32 string subject | Subject preserved |
| Negative signed Int32 string subject | Subject preserved; this does not authorize that identity |
| Unusable expiry/issuer/audience values | These paths still complete; no server policy is inferred |
| Missing subject | Setter/getter complete; subject reader returns an empty nullable result |

The fixtures use deliberately synthetic header/signature text. They are not issued tokens or examples of a verified vendor JWT scheme. Role and privilege claims were absent, and the native calls to the modeled account-flag setters supplied only false flags. Present role-array/string semantics were not exercised.

Two additional cases stopped at native error boundaries: a token with no dot reached the array-bounds boundary; a decoded non-object payload reached the cast-error boundary. Native exception unwinding was not modeled, so these are boundary observations, not exact recovered exception/status behavior.

## Scope of the token contract

The payload helper splits on a dot, reads the second segment, replaces Base64url characters and pads before Base64 decoding, UTF-8 decoding, MiniJSON parsing and a dictionary cast. Its examined body does not read a header or third segment. The subject reader looks up `sub`, requires a string and calls Int32 parsing. The setter's `role` and `rn.privilege` lookups are optional on the exercised paths. Key is separately Base64-decoded and stored with reversible masking; it supplies the signing key rather than defining a JWT signature algorithm.

Text, Base64/JSON parsing, dictionary access, clock, mask generation, buffer pools, permission side effects, nullable construction, metadata/allocation/GC and cleanup helpers are explicit models. In particular, native JSON lexical parsing, native cryptographic helpers and full account construction were not executed. All mapped client data structures were synthetic; no original client data pages were mapped.

The indexed direct callers of the access-token getter are the HTTP header writer, network-auth payload builder, and subject reader. Static inspection finds the network-auth builder copying raw token bytes into its output stream, and the HTTP writer uses the separate request-signing path. This bounded trace does not establish a global absence of checks: unresolved indirect providers, unindexed flows and server validation remain unknown.

The static periodic-refresh state machine waits 3,600 seconds, checks whether stored token bytes exist, then calls the refresh helper. That helper adds a refresh grant name and the stored refresh string to a dictionary and calls the login helper. No refresh-token payload decoding was established there. Another examined method computes elapsed time from a stored login timestamp; it is not an `exp` claim check. These observations establish neither token lifetime nor refresh rotation/replay rules. The response continuation also awaits `SelfAccount` before its later result path; successful payload storage alone does not establish native login.

## Source identity and retained evidence

Source analysis image SHA-256: `a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066`.

| Method | RVA / bytes | SHA-256 of admitted native body |
|---|---|---|
| 149439 | `0x94e2340` / 3514 | `c2e6e1a3cd125fb0405761e4b2decc8b4e60571c6203df35b77d6b0208394ddf` |
| 149440 | `0x94e1a30` / 972 | `ef5e53996016fbbff84f1d681b0ff3ffe4c455e19a08f4b3620f8ecee9341199` |
| 149438 | `0x94e0470` / 681 | `e76245ba41ae67e538a6102c9844465418c3d9d76932d79a7397d1dd16ed807f` |
| 152769 | `0x95520b0` / 867 | `1e3bc7550c2e6c16c5fb7ac72d6c5526e4e07713c44322990462106faa955296` |

Retained Ghidra 12.1.4 dossier IDs are `ev_b107673c0f6738a7dc67cfdbad6c950b288255973bb719840f5592be014b14f0` for 149439 and `ev_b33bda3afb541640fa871d109ab5e031b5996088b13da9864f32b1c982f4cd36` for the 149610 response continuation. Those static dossiers and their limitations are separate from the CPU-guest measurements. No provider evidence IDs are invented for the other hash-checked ranges. The adjacent [sanitized summary](native-token-consumer-20261028-summary.json) records aggregate counts and private artifact hashes only.

## Independent server requirements

Client payload decoding is not authorization. A server adopting JWT/JWS must independently restrict and verify cryptographic algorithms and operations, establish issuer/subject binding, validate intended audience, and avoid trusting received claims merely because they decode. The client trace supplies no verified vendor algorithm, issuer, audience or signature policy. [RFC 8725, sections 3.1, 3.3 and 3.8–3.10](https://www.rfc-editor.org/rfc/rfc8725.html#section-3.1).

Refresh replay protection and rotation remain server responsibilities; the hourly client trigger proves neither. RFC 9700 describes sender constraints or rotation for public-client refresh replay detection and binding issued refresh tokens to approved scope/resources. It also prohibits the resource-owner password credentials grant for modern OAuth deployments. This historical compatibility research does not endorse that grant or claim modern OAuth conformance. [RFC 9700, sections 2.4 and 4.14](https://www.rfc-editor.org/rfc/rfc9700.html#section-2.4).
