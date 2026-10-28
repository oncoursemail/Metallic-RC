# Bounded request-signing profile

`NativeRequestSigningProfile.Compute(decodedKey, pathAndQuery, body)` returns a Base64 HMAC-SHA256 value. It has no transport, credential storage, header attachment, token issuance, route selection, or authentication integration. Existing owned-backend behavior is unchanged.

The retained build 23191908 static evidence links LoginResponse.Key through Base64 decoding in method 149439, masked storage, and unmasking in 149440 to AccessToken.SigningKey. Method 149436 uses that key for HMAC-SHA256. Its observed input is ASCII path/query followed, only for a nonempty body, by its four-byte little-endian signed Int32 length and selected body bytes. Bodies up to and including 2,048 bytes are included in full. Larger bodies contribute sixteen 128-byte slices at offsets `floor(length / 16) * i`, for `i = 0..15`. HTTP method bytes are absent from this signer.

The static request writer uses the literal prefixes `Authorization: Bearer ` and `X-RNSIG: `. Those names are documentation only here. This library does not decide whether a request should receive either header. It does not reproduce URI eligibility, missing-token behavior, expiry, replay protection, or native HTTP serialization.

The profile rejects non-ASCII path/query text rather than guessing the original runtime's encoding fallback. Callers supply its final spelling; no URI normalization occurs. Authored resource limits are 8,192 path characters, 4,096 key bytes, and 16 MiB body bytes. These bounds are not recovered native restrictions. Empty keys and paths are admitted by the pure cryptographic function; this grants no authentication permission. Callers must not mutate input arrays during a call. The function leaves those arrays unchanged.

**Sampling does not establish full-body integrity.** For example, changing byte 128 of a 4,096-byte body leaves this signature unchanged. Do not use this profile as a general authenticated-message format or infer that every body byte was verified.

## Verification

The new standalone C# project passes 71 checks against eight retained synthetic Python-stdlib HMAC vectors: null, empty, 2,048, 2,049, 2,063, 2,064, 4,096, and 65,536-byte bodies. Controls detect full-body hashing in place of sampling, omitted length, big-endian length, and added method bytes. Other checks cover sampled/unsampled mutations, strict ASCII, resource boundaries, null arguments, and input preservation. A retained generator checks that the vector file still matches its independent one-shot Python computation.

```text
dotnet run --project tests/Metallic.RC.SigningProfile.Tests -c Release -p:RestoreLockedMode=true
python -B tests/Metallic.RC.SigningProfile.Tests/generate_vectors.py --check
```

The solution includes this project. The existing backend CI now builds/runs it on Windows and Linux and checks the Python oracle; workflow execution remains pending until CI runs. No existing fixture or test was modified.

These authored vectors establish agreement with the formula and standard cryptographic implementations. A separate private follow-up replayed the exact 546-byte native signer in a bounded Unicorn CPU guest for all eight cases: captured hashing inputs and returned signatures matched. It counted 1,829 native instructions separately from 157 modeled callback entries. Three controls detected guest-memory full-body/omitted-length mutations and a modeled endian mutation. Crypto, ASCII, allocation, conversion and disposal helpers remained models. This replay is separate from CI's authored C# checks; no original-client equivalence, login, account, service, or game success is claimed. No client binaries or private analysis artifacts are included.

The subsequent [token-consumer closure evidence](evidence/NATIVE_TOKEN_CONSUMER_CLOSURE_20261028.md) documents six additional synthetic CPU-guest cases. Neither follow-up changes the signer source or enables authentication.

## Private static provenance

The retained source analysis image identity is SHA-256 `a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066`. The independently checked 149436 range at RVA `0x94d1500` is 546 bytes, SHA-256 `fb68323b14b17a74fa715f4d23c4e1b7c4f20c807c6375434c86240f25b389c4`. Its Int32 conversion target uses a native four-byte x64 store, supporting the little-endian interpretation. The authored implementation contains no copied native body. The full private evidence remains outside this source tree.
