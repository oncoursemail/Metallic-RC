# Native password-form profile, 2026-10-28

`NativePasswordFormProfile.Read` is a pure, bounded decoder for one statically
observed request-body branch in build `23191908`. It does not authenticate a
user, interpret an MFA challenge, issue a token/key, or change the operational
`/recnet/auth/connect/token` route, which still returns its existing HTTP 501.

## Static producer evidence

Offline analysis used the existing restored analysis image, SHA-256
`a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066`, and
normalized metadata, SHA-256
`d675ea92bfb0086c0238e56702e91413fcc663c81fce364253749edbf6fcec9f`.
The client was not executed or attached to. No production service, credential
store, or real account was accessed. Client-secret values and vendor code are
not included here.

| Observation | Method / RVA | Retained analysis evidence ID |
| --- | --- | --- |
| Password helper supplies the seven manual names | 149472 / `0x94DE560` | `ev_89d0884cfa3fe4197dbb8b895832379828a5156a3c8fde53f66772ef8bb72119` |
| Request construction selects POST and the token route | 149600 / `0x9504A30` | `ev_05d0c4def97341c997e57fa3b12f5240d40df20aa48df806c7dad05af5ead9ca` |
| String parameters append to the non-GET body list | 151584 / `0x9537F80` | `ev_8067cd4e5dafb36ce9f02cb6e38fbc3ec872c821f2fd1ff6f55b4925ca4a5bab` |
| The string-list branch explicitly constructs `HTTPUrlEncodedForm` when the binary list is absent | 151611 / `0x95370B0` | `ev_857a5bbdb387fd19aea6e19eb773f866de0b30c8bf6fb76bc2479dc958a3192a` |
| The form applies to the underlying request | 152915 / `0x9554540` | `ev_d905c0d939fbc3eedc9ac02a682da97cba3eaf9b3ed3949178f7658170ab8a0d` |
| Form data joins escaped names and values with `=` and `&`, then encodes UTF-8 | 143900 / `0x93DA220` | `ev_334b468c8d56e31d48f5b5b15d308934839a9beab4c4250058d66f0dcab07f00` |
| Escaping calls `Uri.EscapeDataString`; long strings use chunks | 143901 / `0x93DA0D0` | `ev_dbba1371cf9f3e5b638f95b5e353efcb185d9b4f9848ae61655b3915030fdde0` |
| Header preparation sets `Content-Type` to `application/x-www-form-urlencoded`, without a charset parameter | 143899 / `0x93DA4B0` | `ev_c5e3f5b9769dc411c49d577c66eaefddc2d321027e76bd1d499d9626ac4cfaf9` |

These are source-bound static observations, not an end-to-end native login
capture. Some control-flow continuations remain unresolved. In particular,
`GetEntityBody`'s entry trampoline is unresolved, and the long-string escaping
helper's handling of a surrogate pair across its chunk boundary is not proved.
This patch makes no claim of complete malformed-input parity with the client.

The closed field-name sets match the earlier
[login findings](native-login-20261021/native-login-findings.json):

- Manual: `grant_type`, `username`, `password`, `security_code`, `2fa_code`,
  `login_context`, `require_password`.
- Context: `client_id`, `client_secret`, `platform`, `platform_id`, `device_id`,
  `device_class`, `time`, `ver`, `cid`, `asid`, `locale`, `isInitialLogin`,
  `build_key`.

## Authored decoding and privacy boundaries

The header must match the observed content type exactly. The body must be ASCII
and contain only unreserved bytes or valid percent escapes within each name and
value. A literal `+` is outside this profile; `%2B` decodes to `+`, never a space.
Decoding happens once, with strict UTF-8 and ordinal field-name matching. Decoded
duplicate names and unknown names are rejected. No trimming, Unicode
normalization, case folding, or value reserialization is performed.

These are authored resource limits, not inferred native constants:

| Limit | Maximum |
| --- | --- |
| Encoded body | 65,536 bytes |
| Fields | 32 |
| Decoded name | 64 UTF-8 bytes |
| Decoded value | 8,192 UTF-8 bytes |

A within-profile result requires `grant_type=password` and nonempty `username`
and `password`. Other context fields are optional to the parser; their native
necessity is not established by this component. The four optional manual fields
remain opaque strings, with no security-code, MFA, password-policy, or
authorization semantics.

Public result properties contain status, a static reason code, recognized field
names, and a count. Manual values are accessible only through an explicit
`TryGetManualField` call. Default JSON serialization, serialization as `object`,
public-field serialization, and `ToString()` do not expose those values. A
rejected result exposes no partially parsed manual values or names, including
when rejection occurs after valid credentials. Reason codes never echo an
unknown field name, content type, or value.

Context values are strictly validated from byte scratch storage without
constructing value strings and are not retained in the result, including
`client_secret`. Scratch buffers are cleared on exit. This is not a credential
erasure guarantee: caller-owned input bytes and explicitly returned manual
strings remain in managed memory. The caller must keep the original byte
sequence for any future HMAC use; this decoder neither copies a signing payload
into the result nor produces a normalized replacement payload.

## Local checks

Using .NET SDK `10.0.402`, the new console checks build in locked Release mode
with zero warnings and errors. The local run passed 1,479 synthetic assertions covering all
twenty recognized names; metadata-only serialization; discarded context values;
late malformed/duplicate/unknown fields; percent decoding; strict UTF-8;
Unicode and opaque optional values; exact byte limits; and unchanged caller
bytes on success and rejection. Synthetic credential-shaped values are generated
in memory at runtime. No real credentials are supplied or written to reports.
The unchanged existing login-response self-checks also passed all 106 assertions.
The solution lists the new project successfully, and `git diff --check` passes.

```powershell
dotnet build tests/Metallic.RC.NativePasswordForm.Tests/Metallic.RC.NativePasswordForm.Tests.csproj -c Release -p:RestoreLockedMode=true
dotnet tests/Metallic.RC.NativePasswordForm.Tests/bin/Release/net10.0/Metallic.RC.NativePasswordForm.Tests.dll --self-test NEW-result.json
```

The new project is added to the solution and existing Windows/Linux backend CI
as additive locked-build and synthetic-check steps. Existing tests and fixtures
are unchanged. Independent source and workflow review is required before
integration. CI results and native login acceptance are not claimed by this
local report.
