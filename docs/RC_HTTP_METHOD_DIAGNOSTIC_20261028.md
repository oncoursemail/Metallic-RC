# Native HTTP method diagnostic width

The HTTP observer now reads the method at request offset 24 as an unsigned byte.
The inspected build's typed metadata declares `BestHTTP.HTTPMethods.value__` as
`byte` (field 82660), and the request's method field is at offset 24 (field 82696).
The previous signed four-byte read could include three adjacent padding bytes.
This change affects the emitted diagnostic only.

The new `tools/client-lab/runtime-method.test.js` installs the actual agent's
callbacks in an inert VM and invokes its HTTP observer. Before the fix, the
nonzero-padding GET case reported 2130706432 instead of 0, and POST reported -254
instead of 2. Both zero-padding controls already passed. After the fix, all four
cases pass, preserving input bytes, owned-origin/path reporting, and route count.
The focused agent, readiness, routing, and method suites pass all 55 tests:

```sh
node --test tools/client-lab/runtime-agent.test.js tools/client-lab/runtime-readiness.test.js tools/client-lab/route-policy.test.js tools/client-lab/runtime-method.test.js
```

The existing Windows/Linux backend workflow includes the new method suite in
its Node command. No workflow permissions, events or execution scope changed.

This does **not** fix or explain the recorded access violation. The affected HTTP
callback produced no event during that run. The check uses synthetic memory and
does not establish original-client discovery, login, gameplay, or crash recovery.
No original client was executed for this change. Existing pinned sources and
candidate profiles remain unchanged; this work is isolated from them.
