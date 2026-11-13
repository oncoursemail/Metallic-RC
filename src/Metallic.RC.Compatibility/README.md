# Build 23191908 startup discovery compatibility

This is authored C# for a bounded portion of the recovered client contract, not original game source or a complete backend. The launcher does not silently redirect the game to this component.

`DiscoveryContract.Read` admits valid UTF-8 object/null responses within 1 MiB and depth 64, preserves the 76 observed ASCII aliases, last-property-wins assignment and null/empty-object behavior. Unknown properties are skipped. It uses the .NET JSON parser, not the original Utf8Json parser. Escaped property-name equivalence and arbitrary malformed-input parity are not claimed.

`DiscoveryContract.Write` emits the 32 observed canonical fields in order, including null values. Semantic JSON field/value order is verified, not identical escaping for all strings.

`ApplyResponse` models the observed synchronous service-update stage: reject nonempty status, preserve native service order and earlier side effects on later failure, normalize trailing slashes, preserve `Uri.OriginalString` comparisons, save the initial response, and invalidate/raise on a changed loaded cache. The transport, native async state machine, cache-provider implementation and original exception messages are not reconstructed here. Modern .NET Uri equivalence with the original runtime remains bounded by the test cases. Callers own synchronization around shared dictionaries and callbacks.

Mapping views are read-only. The library contains no native payloads, test mutants, network service, account handling or game patching. Native comparisons used explicit modeled parser/runtime boundaries; consult the associated evidence before broadening the input profile.

Run standalone authored tests (including a four-request ephemeral loopback fixture):

```text
dotnet run --project tests/Metallic.RC.Compatibility.Tests -c Release -- --self-test <new-report.json>
```

These tests require no installed Rec Room, game metadata, REA, Unicorn, live account or remote service.
