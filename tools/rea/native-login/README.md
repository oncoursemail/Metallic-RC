# Build-specific native login analysis

These are retained authored analysis scripts from the private build-23191908 workspace. They do not include game binaries, decompiled game bodies, live user credentials, production client-secret values, or the large fixture outputs.

The exact as-executed workspace on Main is:

```text
%LOCALAPPDATA%\Metallic-RC\rework-20261020\rea-steam-decoding\native-login-probe-20261021
```

The supplied scripts use that existing relative workspace layout and its pinned private inputs. They are not a generic decoder that runs without those inputs. Keep them in the documented analysis layout when reproducing the private native comparison; the public C# self-tests do not need it.

`native_login_probe.py` evaluates the real generated formatter constructor, Serialize and Deserialize instruction ranges in a bounded Unicorn x64 guest. Metadata lookup, allocation, GC barriers, JSON token reading, alias lookup, string encoding and a **null-only nested code-hint interface callback** are declared modeled boundaries. Non-null `code_hint` values stop outside this profile; the real nested formatter is not imitated with invented behavior. Original instruction bytes are hash-bound and not executed directly as host code.

`generate_login_fixtures.py` builds synthetic cases for raw aliases, null/empty objects, strings, wrong types, duplicate fields, unknown fields, and writer ordering. It runs no game, account login, or network request. `compare_managed_login.py` compares the compiled C# profile against those observations and verifies that three deliberately incorrect rewrites are detected. Mutations live in the test harness, not the reusable library.

The selected native spans can cross compiler unwind ranges. The initial grouped REA serializer lookup failed; `focus_login_writer.py` uses a different focused carrier and recovered a serializer dossier. The full 1,232-byte writer extent was independently decoded into 288 instructions with zero invalid instructions. This selected-span result is not a whole-game decompilation claim.

The token/request analyses establish further gates after parsing: token/key processing and an account fetch. They do not provide a verified native token issuer or a complete account-service protocol. Private literal surveys may contain shipped constants; do not publish their raw outputs or reuse any third-party secret against production services.

For game-free reproduction of the authored component:

```text
dotnet build tests/Metallic.RC.LoginContract.Tests/Metallic.RC.LoginContract.Tests.csproj -c Release -p:RestoreLockedMode=true
dotnet tests/Metallic.RC.LoginContract.Tests/bin/Release/net10.0/Metallic.RC.LoginContract.Tests.dll --self-test new-result.json
```

See `docs/evidence/NATIVE_LOGIN_PROFILE_2026-10-21.md` for the exact scope and remaining approval/integration gate.
