# Metallic-owned native token response producer

Claim #372, branch `codex/rc-native-token-response-20261029`, base
`f3b79789f8bf5d8afcca4a193c635da13751a012` (#104). This is an inactive,
in-memory producer. No route, database migration, session activation, key
persistence or native client execution is added.

## Recovered shape and authored policy

The retained build23191908 evidence in #84 establishes that method152769
(RVA `0x95520b0`) decodes the second dot-separated segment as Base64url JSON;
149438 (`0x94e0470`) reads string `sub` as Int32. Method149439 (`0x94e2340`)
separately Base64-decodes `LoginResponse.Key`. The captured bodies do not
establish a required vendor JWT algorithm, signing key, issuer, audience,
expiry policy or exact request-key length. They do not prove a completed login.

This producer makes explicit **Metallic-owned development policy** choices:

- Three-segment HS256 JWT, fixed `alg`/`typ` header, positive Int32 string `sub`.
- Fixed issuer `urn:metallic:rc:local` and audience `urn:metallic:rc:native-local`.
- Explicit whole-second UTC `iat` and `exp`, plus a fresh 128-bit random `jti`.
- A dedicated, caller-supplied 32-byte server signing key. It must come from a
  CSPRNG; length/all-zero checks cannot certify entropy or its provenance.
- An independently generated 32-byte request-signing key, standard Base64 on
  the wire, and a fresh 48-byte opaque Base64url refresh token.
- Access lifetime 1 second to 1 hour, refresh expiry at least access expiry,
  and at most 30 days after the explicit issue time.

The JWT shape follows [RFC7519](https://www.rfc-editor.org/rfc/rfc7519.html).
The 256-bit master key satisfies the HS256 size requirement in
[RFC7518 section3.2](https://www.rfc-editor.org/rfc/rfc7518.html#section-3.2).
Those standards specify our implementation choice; they are not evidence
that the original client requires or accepts these claims or this signature.
Never supply a production/vendor secret, `client_secret`, `admin.key`, or the
request-signing key as the master key. No key file or environment is read.

## Deliberate secret export

`NativeTokenResponse.Create` returns a sealed wrapper whose ordinary JSON is
`{}` and whose `ToString` is static and redacted. It retains neither the
caller's master-key buffer nor mutable body-export buffers.

`WriteTransportBody()` is an explicit credential export. It delegates the
eight-field spelling/order/null handling to `LoginResponseContract.Write`;
the existing formatter is unchanged. Its returned bytes are sensitive and
must never enter logs, evidence artifacts or general metadata serialization.

`UseSessionMaterial(consumer)` deliberately passes the exact access/refresh
strings and expiries as `NativeSessionMaterial`, plus a temporary read-only
span of the independent request-signing key. The callback receives a copy
of that key buffer; even ref-based mutation cannot modify the wrapper's key.
The temporary buffer is cleared on callback return, including exceptions.
Any later reviewed persistence layer must explicitly copy/protect the key
before return. No cryptographic erasure of managed strings or process memory
is claimed. The producer's wrapper remains sensitive in memory.

The callback is not authorization, account verification or a session write.
The existing `CreateNativeSession` still requires real credential checking,
exact durable GUID/native-ID binding and current-expiry admission. The pure
producer uses explicit timestamps and does not assert they equal the present.
This slice does not authenticate decoded claims or add a JWT verification
route. Exact stored-token-hash authority remains separate. A future gateway
must persist and bind the independent request key before emitting any body;
it must not treat construction or serialization as committed login success.

## Synthetic checks

```powershell
dotnet build tests/Metallic.RC.NativeTokenResponse.Tests/Metallic.RC.NativeTokenResponse.Tests.csproj -c Release -p:RestoreLockedMode=true
dotnet tests/Metallic.RC.NativeTokenResponse.Tests/bin/Release/net10.0/Metallic.RC.NativeTokenResponse.Tests.dll --self-test NEW-result.json
```

The checks independently recompute HS256, reject wrong keys and changed
signed content, inspect canonical Base64/Base64url and the existing envelope,
and exercise consumer-style second-segment/string-Int32 parsing. They cover
input/date boundaries, redaction, copied callback/body buffers, callback
failure and observed random-material distinctness. They do not run native
method carriers, accounts, services, endpoints, a game or production keys.
No existing fixture, test, baseline or evaluation was changed.

The cached-only locked dependency restore and Release build passed with zero
warnings/errors. All 117 new synthetic assertions passed locally; no hosted
CI result is claimed for this uncommitted candidate.

Independent source review is required before integration. Key storage,
refresh routes, account-profile admission and actual native-client
continuation remain separate gates.
