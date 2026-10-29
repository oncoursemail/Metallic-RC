# Candidate client-adapter diagnostic mode

Board claim 328. Branch: `codex/rc-client-adapter-mode-20261029`, based on the source from PR 95. This is a diagnostic source candidate. No active profile was changed, and no native test or elevation was requested for this change.

The optional manifest field `clientAdapterMode` accepts exactly `full` or `network-only`. Omission preserves the existing `full` behavior. Other values and types fail before pinned inputs are admitted; the worker independently validates the mode. Candidate preparation preserves a valid existing mode, defaults an omitted mode to `full`, and supports an explicit reviewed choice through `--client-adapter-mode`. It still requires a new output file and verifies the previous input pins. Tests use authored temporary manifests and inert files only.

`network-only` skips this agent's managed string-constructor, URI-constructor, HTTP-observer and discovery-observer hooks and their prefix polling. It retains the same Winsock, DNS and dynamically discovered ConnectEx guards, including export preflight, alias rejection, loopback-only destinations, registration/flush checks and stopped-state handling. The `installNetwork` implementation and route policy are unchanged. It does not set `fixtureOnly` for an actual client: that remains explicitly false. Client-adapter readiness remains false in this mode; network-guard readiness is independently required before resume.

Mode identity is passed in the injected configuration and recorded in agent status, the agent-loaded event, worker receipt, supervisor record and summary. The worker refuses to resume on absent guards, a stopped agent, missing admission, or an agent mode that differs from the requested mode. The target executable, original-file pins, arguments, working directory, allowed environment, UAC requirement, deadlines, containment and cleanup remain unchanged.

`network-only` cannot promote a discovery-shaped event into native-discovery success. It keeps result 2 when there is no discovery proof, preserves actual process exit/cleanup facts, and makes no login or room-join claim. Since original non-loopback endpoints remain blocked and no managed URI rewriting occurs, this mode is a causal startup control, not a way to complete backend login. An absent HTTP event does not prove that a callback was never entered; a fault can occur before its event is emitted. The earlier recorded Frida exception-handler prologue fault does not establish the original trigger.

Validation commands:

```powershell
node --test tools/client-lab/route-policy.test.js tools/client-lab/runtime-agent.test.js tools/client-lab/runtime-readiness.test.js tools/client-lab/runtime-method.test.js tools/client-lab/runtime-client-mode.test.js
python -B -m unittest discover -s tools/client-lab -p 'test_*.py' -v
```

Actual-agent VM checks verify both full modes, network-only guard installation, rejection of partial guard installation, ConnectEx deduplication/alias rejection, destination/DNS denials, no managed reads or native-constructor calls, and invalid-mode refusal. Authored manifest/worker checks cover strict mode validation, no fixture mislabeling, full-default behavior, refused resume, mode mismatch, result 2 despite a synthetic discovery-shaped event, and preservation of synthetic `0xC0000005` exit facts. These are offline controls, not a new original-client run.

Results on this candidate: 87 JavaScript tests passed; all 81 Python unittest methods passed in 24.014 seconds. The Python suite includes authored Windows containment controls, with no original-client process. `git diff --check` passed. A direct comparison against base `c102e2e` confirms the entire `installNetwork` function is unchanged (normalized-text SHA-256 `5bd7c1832b8a5dfbb894d39a27db58b54b13ef78edf655d90f6ac35672cf5a3d`).

Independent peer review passed: all 87 JavaScript tests plus 10 adapter-mode, 18 profile-refresh and 15 startup-output Python tests passed. The parent separately reran all 81 Python tests and 87 JavaScript tests. The peer confirmed unchanged Winsock guard source and the preserved target, environment, deadline, job and UAC controls. Neither reviewer ran the original client for this change.

Data-only candidates may now be prepared for separate review. Any eventual native test requires its own owner approval; earlier one-run approvals do not authorize another attempt.
