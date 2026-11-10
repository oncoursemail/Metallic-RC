# Base-file decryption — reverse-engineering status

Target: the owner's own 2026 Rec Room build (20260323.14, Unity 6000.0.27f1, IL2CPP).
Scope: Metallic-RC preservation/interop. All extracted/decrypted game data stays LOCAL
(outside the repo). This document records only our own analysis.

## Problem
The on-disk Unity base files — `globalgamemanagers`, `level0..113`, `resources.assets`,
`sharedassets*.assets`, and some `.resS` — are encrypted: each begins with an 8-byte tag
`00 00 52 52 00 00 52 52` ("RR") and a high-entropy body. Standard Unity tooling can't read them,
so the built-in scenes (every room/map) were previously inaccessible; the Rust client worked around
this by hand-authoring only the Dorm layout.

## Cipher characterization (confirmed)
- Body = `file[8:]`; ciphertext length == plaintext length (no block padding) ⇒ **stream cipher**.
- Body entropy ≈ 8.0 bits/byte; keystreams are **per-file** (uncorrelated across files) and
  non-periodic.
- A decrypted file is a valid Unity SerializedFile whose legacy header `file[0:8]` is all-zero, so
  `ciphertext[0:8] == keystream[0:8]`; that it equals the marker is a consequence of the zero header,
  not an independent constant.
- Empirically ruled out (offline, against recovered keystreams): RC4(filename); RC4/AES-CTR with
  key = SHA-256/MD5(filename or asset path) variants; small-seed (<2^20) MSVC-LCG, glibc-LCG,
  xorshift32. ⇒ there is an **embedded master key**; the per-file keystream is derived from the
  filename (the only per-file input available to an on-the-fly read-time decryptor).

## Where the decryptor lives
- `nodeline.dll` (revival loader): BCrypt **SHA-256 only** → build fingerprinting, not the decryptor.
- `UnityPlayer.dll` (custom "Metallic" build, 32 MB): bundles **mbedTLS** (SHA-256 K-table present;
  "AES", "RC4", "decrypt"/"Encrypt", "Metallic" strings). No AES T-tables / S-box / ChaCha sigma /
  marker immediate are present statically ⇒ a constant-free cipher (RC4 or AES-NI/CTR) and/or the
  decrypt is **wired up at runtime**: the kernel32!`ReadFile` interception that transparently
  decrypts `RecRoom_Data` files only becomes active **after engine init**.
- Full-image Ghidra analysis of the 32 MB UnityPlayer times out; reversing the routine needs a
  targeted (offset-seeded) static pass or a runtime capture.

## Current decryption capability
- All 230 encrypted base files have been decrypted to a LOCAL corpus via a **runtime oracle**
  (reading each file back through the game's own active decrypt hook). This is reproducible but
  requires the game running; it does **not** yield the algorithm.
- With that corpus, our `rust/unity-assets` readers parse **all 114 built-in scenes** (scene paths,
  full hierarchy roots, per-class object counts) — i.e. every built-in room/map is now readable.

## Remaining work
1. **Reverse the exact native cipher + master key + per-file key derivation** (runtime crypto
   capture of the startup `globalgamemanagers` decrypt, or targeted static RE of the UnityPlayer
   read hook), then implement it offline as a Rust `decrypt` module with tests against known pairs.
2. Per-environment load data for every scene from the corpus (replacing hand-authored layouts).
3. Coverage report.
