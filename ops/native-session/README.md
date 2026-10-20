# One private native session

[`run.example.sh`](run.example.sh) runs the [session supervisor](../../services/session-runner/README.md) in the foreground. It installs no daemon, container, firewall rule, listener outside loopback, or global service. Run it as the intended unprivileged account.

Set these variables to actual, reviewed local paths and the independently checked manifest hash:

| Variable | Meaning |
| --- | --- |
| `METALLIC_RC_CHECKOUT` | Absolute repository checkout |
| `METALLIC_RC_BUILD_MANIFEST` | Absolute `BUILD.json` inside a native `server` export |
| `METALLIC_RC_DATA_DIR` | Absolute private session directory; existing parent required |
| `METALLIC_RC_MANIFEST_SHA256` | Exact SHA-256 of the reviewed manifest |
| `METALLIC_RC_SESSION_PORT` | Optional fixed loopback port; default `27841` |

Then run `bash ops/native-session/run.example.sh`. Use Ctrl+C to stop the supervisor and its owned child. Check `receipt.json` in the data directory for the final status; it must report `stopped` after an orderly operator stop. The native progress file stays in that directory for the next run.

This example is intended for local, controlled validation on hardware with measured headroom. It does not authorize a public deployment or imply that issue #67's signed builds, distributed fencing, host isolation, tickets, or fleet tests are complete.
