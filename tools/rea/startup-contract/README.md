# Startup response reconstruction and verification

The reusable module is `src/Metallic.RC.Compatibility`, integrated into the solution and the launcher's offline `check-discovery` command. Its standalone tests need no game files or REA:

```text
dotnet run --project tests/Metallic.RC.Compatibility.Tests -c Release -- --self-test result.json
```

This folder preserves the authored exact-build native-verification sources and the deliberate negative-control probe. **Negative-control hooks are not in the reusable library or launcher.**

The native verification scripts are retained with their executed workspace layout: on Main they run from `.../rea-steam-decoding/startup-contract-fix-20261021`, beside the captured run, private SQLite atlas and the existing private Unicorn vendor directory. A source-only checkout does not contain those game files. The scripts hash-pin the actual input image and refuse unknown execution targets. Do not copy these analysis outputs over Steam files or run analysis carrier DLLs.

`native_contract_probe.py` evaluates the exact formatter constructor/read/write instruction ranges. Allocation, metadata, GC barriers, token reading, dictionary lookup and string encoding remain explicit modeled boundaries. The comparison checks generated formatter decisions, not all original Utf8Json or runtime behavior. `verify_cleanup.py` evaluates the omitted cleanup block and the exact RET 0 disposal leaf; it stops before the native exception propagation implementation.

The public library removes test mutants, exposes read-only mapping views, admits a bounded JSON profile, and has no implicit game redirection or network server. The loopback test is an authored four-request fixture, not a live game or official-service connection. See `docs/evidence/REA_STARTUP_CONTRACT_FIX_2026-10-21.md` for results and remaining gates.
