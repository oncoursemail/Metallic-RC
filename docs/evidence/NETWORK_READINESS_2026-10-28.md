# Native laboratory network readiness candidate

28 October 2026. Candidate source on `codex/rc-network-readiness-20261028`, based on `00aa4ac`. Not deployed; existing test profiles, manifests, installed binaries and services are unchanged. Independent review is required before integration or a new pinned profile.

The previous agent could mark Winsock guards installed after silently skipping absent exports. It also set that marker before `Interceptor.flush()`. A loaded Winsock module therefore did not establish that the expected registrations had completed.

The candidate resolves all eight existing required exports before installing a hook, rejects missing/null or overlapping locations, and stops on wrapper, replacement, observer or flush failures. Six existing replacements and the `WSAIoctl` extension observer must finish and flush before network readiness is published. A stopped agent reports network guards unavailable. The same socket/DNS APIs, callback signatures, loopback admission rules and error values remain in use.

The existing dynamic ConnectEx path now rejects a null function address or an address aliasing any of the eight required exports, and only emits its installed event after a new registration successfully flushes. Repeated results for a genuine ConnectEx address do not reinstall or emit a second installed event. A ConnectEx registration/flush failure stops the agent and revokes readiness. Partial registrations are not described as ready; no new rollback or instrumentation technique is introduced.

Executed:

```text
node --test tools/client-lab/runtime-readiness.test.js tools/client-lab/runtime-agent.test.js tools/client-lab/route-policy.test.js
node --check tools/client-lab/runtime-agent.js
git diff --check
```

Result: **51 tests passed, zero failed or skipped**: 41 new readiness tests and 10 unchanged agent/policy tests. The new suite executes the actual agent in a JavaScript VM with inert Frida-shaped mocks. It covers every missing/null required export, alias/lookup errors, registration/construction/flush failures, complete registration ordering, repeat-module and ConnectEx deduplication, and unchanged socket/DNS admission and error values. Independent review identified the dynamic-extension alias gap; eight further cases now reject ConnectEx aliasing each required export before replacing another function.

For regression evidence, the same new readiness suite also ran against the unchanged agent from `git show 00aa4ac:tools/client-lab/runtime-agent.js` in a fresh private scratch directory. The mock supplies both Frida export-lookup methods, including the throwing `getExportByName` used by that baseline. **12 passed and 29 failed out of 41**, with zero skipped. Failures include absent socket/DNS/extension exports, readiness before a successful flush, an unusable observer, and ConnectEx installation/aliasing/deduplication. The baseline was not edited or deployed. The current three-suite run passed **51 tests**.

These are authored control-flow checks. They do not load Frida, hook a process, open a socket, launch the original client, change security settings or prove original-client discovery/login/gameplay. The existing guards remain a bounded laboratory mechanism, not a complete operating-system network sandbox. Native integration and a real Frida regression remain separate acceptance gates.

A separate reviewer found an unsafe dynamic-address deduplication case during
review. It was repaired before publication: a ConnectEx address matching any of
the eight preflight exports now stops the agent rather than reusing or replacing
an incompatible callback. The reviewer independently reran all 51 final tests
and reported no remaining blockers in this scope. The existing backend workflow
runs the new suite alongside its unchanged policy and diagnostic suites.

[Frida JavaScript API](https://frida.re/docs/javascript-api/) documents export
lookup, replacement, module observers and flushing. These API contracts inform
the implementation; mocked results do not prove their behavior in the original
game process.
