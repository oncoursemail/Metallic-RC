# Referee fault: recorded bytes and remaining uncertainty

The network-only native test crashed in Referee.dll 2.1.1.1 at RVA `0x62bb0b`.
Its installed SHA-256 is
`5868c90a7c67757f3b678aa356cbd933d646c9749e0acaa31b7dcc9eaee12ead`.
No managed-client hooks were enabled for that test. The earlier full-mode test
reported its final fault in Frida's Windows exception-dispatch prologue.

The Referee fault address has no authoritative mapping to stored DLL bytes.
Actual REA/Ghidra 12.1.4 analysis confirmed this; decoding a guessed file offset
would not explain the recorded runtime. Existing local Windows minidumps from
both approved tests were then found and inspected offline. No new native run,
elevation, protected-archive query or live memory access was needed.

## Observed instruction

A bounded reader matched dump PID, time, exception, module and RVA, selected the
exception stream's context and recovered 128 captured instruction bytes.
Independent Iced and REA/Ghidra decoding agree on the fault instruction:

```asm
mov dword ptr [rax + r11], 0x2d
```

The recorded registers compute exactly the write address in the exception
record. This establishes what failed; it does not explain how that index and
pointer state arose.

REA Evidence:
`ev_394c3220e9a5cf6c41a883f2a0e10f677246c97339ccce97f748369ba6b9dbe3`.
Captured-code SHA-256:
`32e5bc0f7f6f73cd3fb7421be43de2937a2f693aa514039161eca38a4a1de842`.

Windows REA did not support raw x64 input in this provider profile. The successful
analysis used a separate, clearly labeled synthetic PE carrier of the exact
captured bytes at their recorded address. Its headers and padding are analysis
aids, not vendor-image evidence. It was never executed. Ghidra inferred a
129-byte body containing one synthetic INT3 beyond the 128-byte capture; that
instruction is excluded. Function boundaries, full control flow, parameters and
calling convention are not established.

## Recorded thread state

Selected NT_TIB fields show the Referee exception thread's stack pointer only
992 bytes above its recorded StackLimit. The older Frida exception thread's
stack pointer equals its recorded StackLimit; its faulting PUSH writes eight
bytes below that limit. Both 64 KiB stack samples repeatedly contain the same
Referee fault address, along with other exception-dispatch-related module
addresses. These are aligned pointer candidates, not an unwound call stack.
Repetition and stack pressure support further exception-handling investigation
but do not prove recursion, an instrumentation conflict or an original cause.

The field layout follows Microsoft's [NT_TIB64 definition](https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/SystemServices/struct.NT_TIB64.html).
Frida's corresponding release has a [Windows vectored exception dispatcher](https://github.com/frida/frida-gum/blob/17.24.3/gum/backend-windows/gumexceptor-windows.c).
These references explain the structures and mechanism; they do not establish a
symbol-level match or diagnose the vendor component.

## Reader validation and limits

The selective fault reader passed 33 authored synthetic tests with independent
review. A separate bounded stack reader passed 12 tests and independent review,
including a reproduced partial-mapping conflict that was fixed before real
use. The original dump hashes matched before and after analysis. Their payloads
remain private; no dump, vendor binary, full log or code carrier is published.

Only the verified AMD64 integer/control prefix is interpreted. The third-party
parser's unused union/vector parsing is flawed and is neither validated nor
emitted. The analysis avoids PEB/environment parsing and heap searches; whole
files are read for provenance hashes. Stack extraction is capped at 64 KiB and
emits only module-relative candidate counts.

No normal-launch baseline without instrumentation has been established. No
vendor DLL patch or verified startup repair exists. The accompanying source
change repairs a separately reproduced false-success bug in our test harness;
it does not repair Referee. Native discovery, login, gameplay and voice remain
unverified.
