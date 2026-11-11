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

## UPDATE 2 — devirtualization attempt (four independent approaches, all blocked by the VM)

A focused attempt to recover the exact cipher for a pure-Rust reimplementation. Every independent
approach hits the same barrier:

1. **Keystream cryptanalysis** (337 recovered keystreams, `enc XOR dec`): the keystream is
   cryptographically strong — no block repetition at any boundary (16/32/64/256/512 B), all
   per-file distinct, uniform byte distribution with no RC4-style positional bias. So it is
   AES-CTR-class / hash-PRG output; no statistical shortcut exists. (AES itself already ruled out:
   no key schedule in the 2.2 GB dump, no AES-NI, no constants.)
2. **Static disassembly of the hot blocks** (live-dumped from the running process): the dispatcher
   at `+0x6abb80` is a VM opcode switch; `+0x2190xxx` (once/512 B) is a **spinlock with a
   KUSER_SHARED_DATA timeout** (`gs:[0x30]`, `0x7ffe0000`, `lock cmpxchg`, `pause`) — anti-cheat
   thread synchronization, not cipher. Handlers spill all state to the stack and split memcpys into
   odd chunks (virtualization signature).
3. **Runtime Stalker trace** of one decrypt: 16,362 Referee blocks; the hottest are the VM
   dispatcher and the spinlocks, i.e. the trace is dominated by VM-dispatch + synchronization
   overhead, not readable cipher ops.
4. **Runtime memory-read capture** (minimal 64-byte decrypt, per-instruction callouts): reads are
   dominated by **stack spills** (one stack page read 4,764×) and **runtime-allocated regions**
   (`0x185xxxxxxx`/`0x186xxxxxxx`); Referee's static image is essentially not read (2 reads). The
   cipher executes as VM bytecode interpreted from heap, so a fixed master-key region cannot be
   separated from the repeatedly-read VM bytecode by access frequency.

**Conclusion:** the base-file cipher is protected by production-grade **code virtualization +
anti-cheat synchronization**, with the cipher logic as heap-resident VM bytecode and all working
state on the stack. Recovering the exact algorithm + embedded master key requires **full VM
devirtualization** (reconstructing the VM's opcode semantics and lifting the bytecode) — a dedicated
research-grade effort (akin to writing a VMProtect/Themida devirtualizer), not something to brute
force inline. It is **not** required for usability: the runtime-oracle **corpus is complete
(337/337)** and every scene parses, so on-the-fly decryption is realized as a decrypt-once cache
(`rebuild_corpus.py`). If a standalone offline decryptor is genuinely needed, scope VM devirt as its
own project.

## UPDATE 3 — VM devirtualization (commissioned by the owner): progress

Owner commissioned the full devirt. Steady, checkpointed progress (all analysis local in
`rr-capture/decomp-full/work/native/`):

- **Handler jump table recovered** at `Referee+0x6b1de8`: 443 opcode entries → 338 distinct handler
  RVAs (106 opcodes route to a default no-op at `+0x6b1dde`); handlers are uniform ~73-byte case
  blocks of the switch at `+0x6abb80`.
- **Store primitive fully lifted** (`vm_store_opcodes.json`, 336/443 opcodes): each opcode is a
  scattered byte-store into a **2 MB (2^21) VM memory space**:
  `VM_mem[(base[op] + index) mod 2^21] = (byte + transform[op]) mod 256`, with a distinct per-opcode
  base (range `0x56b`..`0x1fed39`) and an additive per-opcode transform (161 `sub`, 175 `add`
  constants). This is a MOV-obfuscation: the opcode encodes both a scrambled memory region and a
  byte-level correction, so the VM's working variables live scattered across the 2 MB space.

### Still to do (the long tail)
1. Lift the matching **load primitive** (read byte + inverse transform) and the **arithmetic/logic
   opcodes** (the actual cipher ops between VM variables).
2. **Trace-based reconstruction**: instrument store+load at runtime, normalise each (op,index,byte)
   via the lifted tables into (addr,value) over the 2 MB space, and reconstruct the data flow of one
   decrypt → the keystream generator, the per-file (filename) key derivation, and the embedded
   master key.
3. Validate offline byte-for-byte vs the corpus; implement real `decrypt()` in Rust.

This remains a multi-week effort; the corpus is the shipped solution meanwhile.

## Done
- Per-environment load data (`scene-manifest`, `EXTRACTION_COVERAGE.md`); coverage report; 100%
  asset extraction (0 reader failures); corpus 337/337 with `decrypt.rs` single entry point.
