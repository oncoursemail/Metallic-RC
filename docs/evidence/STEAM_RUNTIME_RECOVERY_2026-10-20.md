> **Later checkpoint:** native types/generics, typed C# declarations, conservative leaf-body translations, and syntax-checked source are recorded in [the C# reconstruction report](CSHARP_RECONSTRUCTION_2026-10-21.md). The original capture results below remain historical evidence.

# Steam Rec Room: stable metadata and native-code recovery

**Observed on Main, 20 October 2026.** Status: normalized IL2CPP metadata passes an independent upstream parser; captured native code is restored for offline analysis, and all metadata methods have a pointer-table disposition. This is not original C# source, a complete Unity project, a runnable recompiled game, or a finished Metallic-RC server.

This report supersedes the earlier “metadata not yet recovered” checkpoint without removing the earlier failures. The previous download comparison found all 3,298 files identical between the installed client and the newly downloaded Steam depot. This work therefore used the same verified build rather than inventing a newer client.

## Exact build and successful capture

| Item | Result |
|---|---|
| Steam app / Windows depot | 471710 / 471711 |
| Build / manifest | 23191908 / 8867414017562658247 |
| Unity | 6000.0.27f1, Windows x64 IL2CPP |
| Stable runtime metadata | 52,403,096 bytes, complete read |
| Runtime GameAssembly image | 291,041,280 bytes, complete read |
| Stable metadata SHA256 | `5af86c2e051811bf84f56461af8757bc767855645aa4ef8985487a3333ac71e4` |
| Runtime image SHA256 | `8f69ea448097cfffb3e3a4e61eb92981db99c14c3e1fbaba572afbaf8a63a732` |

The capture bound to the exact original Steam executable and the wrapper-owned PID. Windows administrator approval was granted through the normal UAC prompt. It used query/read process access, not memory writes or injected code. The successful run began at 21:25:03 UTC; the candidate metadata remained byte-identical across repeated reads spanning 0.628 seconds. Sixteen sample windows across each of the two protected code sections passed the capture heuristic. Entropy and repeated hashes are capture checks, not a proof that every game function has been semantically reconstructed.

The correction was in our capture timing: after finding the exact-size candidate allocation, the helper rereads it directly instead of rescanning the entire process address space. It also saves the first structurally plausible read as provisional before demanding stability. That prevents a promising read from being discarded when the original client exits soon afterward.

The wrapper closed its temporary section handle and cleaned up only its own retained process handles. SHA256 checks after analysis matched both the installed and fresh copies of RecRoom.exe and GameAssembly.dll. The fresh download was not launched. No game payload, DNS configuration, registry setting, or security setting was changed by the analysis scripts.

## The apparent crash in the preceding run

The retained Player log from the previous v3 run shows a name-server DNS lookup failure for `ns.rec.net`, followed by `Running => PreparingToExit`, `ReadyForExit`, and `Application.quit`. That run recorded exit code 0. It is evidence of a controlled shutdown after a failed service lookup, not evidence of a corrupted Steam payload.

This diagnosis is limited to that observed run. It does not claim that every crash reported by the owner has the same cause, nor that public service connectivity is fixed. See the selected source lines and log digest in [startup-exit-evidence.json](steam-recovery-20261020/startup-exit-evidence.json).

## Normalized metadata

The 64 header integers were resolved into one complete tiling of 31 table regions. Roles were then checked through record layouts, tokens, bounded string indices, reciprocal assembly/image links, complete type/member ownership, method/parameter ownership, generic-container relationships, assembly-reference ranges, literal ranges and attribute sentinels.

The normalized derivative restores a standard v31 header and repairs two documented fields in each of 418 module placeholder records. One module has a legitimate static constructor; it is preserved. Exactly 2,308 bytes differ from the captured source, restricted to the header and listed repairs. The raw capture remains unchanged. Empty WinRT tables, alignment padding, unnamed zero-row-token parameters and the synthetic generated image are preserved explicitly rather than mislabeled as corruption.

| Metadata inventory | Count |
|---|---:|
| Images / assemblies | 419 |
| Type definitions | 63,648 |
| Method definitions | 409,503 |
| Fields | 252,928 |
| Parameters | 373,383 |
| Properties | 80,911 |
| Events | 1,802 |
| Generic containers / parameters | 8,922 / 14,086 |
| String literals | 83,658 |

These totals include Rec Room, Unity, framework and other bundled assemblies; they are not all game-authored classes.

**Independent check:** unmodified Perfare/Il2CppDumper metadata parser source at commit `4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a` parsed the result as v31, produced the same counts and completed 1,260,747 identifier lookups. The parser received a private memory copy, and that input memory remained unchanged. Source URLs, hashes and the MIT license are retained with the verifier. [Independent parser result](steam-recovery-20261020/independent-parser-result.json).

The normalized file is `global-metadata.v31.normalized.dat`, SHA256 `d675ea92bfb0086c0238e56702e91413fcc663c81fce364253749edbf6fcec9f`.

## Native code and method mapping

The offline derivative restores 194,843,984 bytes of captured `.text` and `il2cpp` code. Its `.rdata`, `.data`, `.pdata` and `.reloc` disk sections remain byte-identical to the original. The relocation table was checked; no code relocations needed undoing. Only documented analysis header fields were adjusted. This derivative is not intended to load or run.

`GameAssembly.code-restored.analysis.dll` is 287,903,264 bytes, SHA256 `a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066`.

Module structures were identified by exact image-name pointers, count bounds and executable method-pointer ranges, not by assuming a module-array order. All 417 nonempty module counts equal their exact highest metadata method-token row. The remaining two images have zero methods.

| Mapping result | Count |
|---|---:|
| Method definitions accounted for | 409,503 |
| Non-null native method-pointer entries | 367,865 |
| Distinct native addresses among those entries | 270,148 |
| Null method-pointer entries retained explicitly | 41,638 |

Shared native addresses are not counted as distinct functions. Null pointers may represent abstract/generic or otherwise non-direct methods; no bodies were invented. This does not yet normalize every native type, generic instantiation, registration or method signature. [Method-map verification](steam-recovery-20261020/method-map-verification.json).

## Actual REA results and their limits

REA 6.5.0 with Ghidra 12.1.4 was run against the restored full DLL, but its import reached `provider_timeout` after approximately 335 seconds. That failure is retained and is not relabeled as success.

A bounded follow-up placed exact, SHA256-verified source code pages at their original virtual addresses in a generated analysis PE. Two name-server accessors produced five-byte bodies with readable assembly and pseudocode and no decompiler warnings. The Paintball initialization getter produced its complete 116-byte body; adding the verified Unity inequality tail-call target removed a boundary-induced bad-instruction warning. It still has unresolved global/type information, and the inferred void return is not accepted as the original source signature.

| Selected method | REA evidence ID |
|---|---|
| NameServerResponse.get_Auth | `ev_c95434eacee17375356ef42d2b5300e690017d7c141e126ddf5b5cd07a0f3ae1` |
| NameServerResponse.get_API | `ev_58451cf53a49adeb95d4b2dfd6e6f5adee554fa0bd5f500ce34aec33baf530cd` |
| PaintballManager.get_IsInitialized, with tail target | `ev_277e81f78606d7dc2547f20b1c5710039c594671b5026e6b9a05a68406e9737e` |

The PE headers and export labels in these small carriers are generated scaffolding. The selected instruction bytes match both the captured image and the restored derivative. Out-of-page dependencies remain unresolved. These are selected-function observations, not proof that the whole game was decompiled. Complete canonical REA records, source-range provenance, assembly and pseudocode remain in the private `decoded/rea` folder. [Scoped check summary](steam-recovery-20261020/rea-function-checks.json).

## Tests and preservation

Seven capture-helper tests and three readiness checks passed. Seven negative metadata tests rejected deliberately corrupted method tokens, name indices, method ownership, literal ranges, attribute sentinels, image links and module shapes. [Negative-test results](steam-recovery-20261020/metadata-negative-tests.json).

The report, aggregate evidence and analysis-tool source are retained in the project. Captured binaries, normalized game metadata, complete identifier/type/method indexes, and decompiled game snippets are not committed to GitHub. This checkpoint does not change the launcher implementation or declare a release. Main remains unchanged unless a separately reviewed merge occurs.

## Files on Main

All recovered artifacts are under:

```text
C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding\runs\recovery-\20261020T212503.738773Z\decoded
```

Important files: `global-metadata.v31.normalized.dat`, `GameAssembly.code-restored.analysis.dll`, `assembly-index.json`, `upstream-parser/types.jsonl`, `upstream-parser/methods.jsonl`, `native-method-map.jsonl`, `RECOVERY-SUMMARY.json`, and `rea/`.

The source capture and complete read-coverage reports are in the sibling `capture` directory. The normalization report records every changed byte field. Do not replace Steam's installed DLL or metadata with these analysis derivatives.

## Next acceptance gate

Complete the native type/generic registration reconstruction and obtain trustworthy signatures for the modern client's startup/service boundary. Then connect the actual client to a version-matched, owned backend and demonstrate two clients sharing room state. Asset/project reconstruction, voice/reconnect, in-game CV3, native ports and performance measurements remain separate work. The current milestone is usable decoded metadata and native-code evidence—not a finished replacement game.

## Primary tool references

- [Pinned independent metadata parser](https://github.com/Perfare/Il2CppDumper/tree/4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a/Il2CppDumper).
- [REA 6.5.0 native Windows boundary](https://github.com/morluto/rea/blob/rea-agents-6.5.0/docs/windows-ghidra-p0.md).
- [Exact-build recovery notes used as hypotheses, then checked against this capture](https://github.com/VT-0xFF/RecRoom_Info/blob/main/DUMPING.md).
