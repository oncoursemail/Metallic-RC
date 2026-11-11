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

## UPDATE (runtime trace) — the decryptor is virtualized anti-cheat code

Correction to "Where the decryptor lives": it is **not** UnityPlayer/mbedTLS. Traced from the
process dump and confirmed by a live Frida capture:

- `ntdll!NtReadFile` is **inline-hooked**; the hook handler is in **`Referee.dll`** (Rec Room's
  63 MB anti-cheat) at `+0x223f6c0`. It decrypts matching `RecRoom_Data` reads transparently.
- A live Stalker trace of one fresh `levelN` decrypt (attach to the running game, drive one read
  through the hook on our own thread, follow the handler) shows the work happening entirely inside
  `Referee.dll`: a hot dispatcher at **`+0x6abb80`** that reads an opcode id (compared to
  `0xC9E`/`0xC9F`/`0x1BA`…) and jumps through a handler table at `+0x6b1de8`, with all state spilled
  to the stack and memcpys split into odd byte-chunks — i.e. the decrypt is **code-virtualized**
  (VMProtect/Themida-style). Block-setup runs ~once per 256 B and 512 B (position/block structured).
- No standard primitive is present (no AES key schedule anywhere in the 2.2 GB dump, no AES-NI, no
  AES/SHA/ChaCha constants); it is a bespoke per-file-keyed stream cipher expressed as VM bytecode.

**Conclusion:** a faithful, offline, pure-Rust reimplementation of the cipher is **not feasible
without devirtualizing the Referee VM** (a large, specialized effort out of proportion to its value
here). The complete decrypted **corpus already exists** (230 files, produced by the runtime oracle)
and every built-in scene parses from it, so all downstream work is delivered without reimplementing
the cipher.

## Implemented
- `rust/unity-assets::decrypt` — marker detection (`is_encrypted` / `file_is_encrypted`), and
  corpus-backed resolution (`read_base_file`, `plaintext_dir`) so readers transparently consume the
  decrypted corpus for an encrypted install. (Pure-Rust in-process decryption is blocked by the VM,
  as above; the module documents this and the oracle that produces the corpus.)

## Remaining / optional
1. (Optional, large) Devirtualize the Referee VM to recover the exact cipher for standalone offline
   decryption. Not required for usability — the corpus covers it.
2. Per-environment load data for every scene from the corpus (replacing hand-authored layouts).
3. Coverage report.
