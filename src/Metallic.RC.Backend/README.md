# Metallic-RC owned backend

A running, persistent C# backend for Metallic-owned accounts, profiles, room records, and a bounded live room presence/pose protocol. It also serves the recovered **Steam build 23191908 discovery-response shape** through the existing compatibility library.

**This is not yet the original Rec Room client's complete backend.** Native login, native room admission, gameplay transport, voice, and actual-client acceptance remain unimplemented/unverified. The owned `metallic-v1` endpoints are explicitly separate from native `/recnet/` routes. Unimplemented native routes return **501**, not fake successful payloads. A valid discovery response does not make all 31 advertised services implemented.

## Run locally

```powershell
dotnet run --project src/Metallic.RC.Backend -c Release -- --port 4187 --data-dir "$env:LOCALAPPDATA\Metallic-RC\backend\data"
```

Use a new, absolute data directory. The host binds only **127.0.0.1**, even when a machine has external ASP.NET endpoint configuration. It does not alter Steam files, DNS, certificates, security software, firewall rules, or Metallic's public domain. Existing local authoring on port 4186 remains separate.

`GET /healthz` identifies the server. `GET /readyz` distinguishes an available owned service from native-client readiness. `GET /metallic/v1/capabilities` states both implemented features and missing native compatibility.

## Interface

| Endpoint | Behavior | Authentication |
|---|---|---|
| `GET /`, `/nameserver`, `/compat/23191908/nameserver` | Canonical 32-field discovery JSON; 31 local service addresses | None; loopback only |
| `POST /metallic/v1/auth/register` | Create a Metallic-only account | Local administrator key |
| `POST /metallic/v1/auth/login` | Verify password and issue a one-hour opaque session | Username and password |
| `POST /metallic/v1/auth/logout` | Revoke session and disconnect its room connections | Bearer session |
| `GET /metallic/v1/me` | Read own profile and ETag | Bearer session |
| `PUT /metallic/v1/me` | Change own display name with `If-Match` | Bearer session |
| `GET /metallic/v1/rooms` | Public rooms and rooms owned by the caller; maximum 200 records | Bearer session |
| `POST /metallic/v1/rooms` | Create a persistent room record | Bearer session |
| `GET /metallic/v1/rooms/{id}` | Read an accessible room | Bearer session |
| `GET /metallic/v1/rooms/{id}/socket` | Upgrade to the owned room protocol | Bearer session in Authorization header |
| `GET /metallic/v1/admin/diagnostics` | Bounded hashed-route counters and live connection count | Local administrator key |
| `POST /metallic/v1/admin/backups` | Consistent SQLite backup with integrity/hash verification | Local administrator key |
| `POST /metallic/v1/admin/shutdown` | Graceful host shutdown | Local administrator key |
| `/recnet/{service}/...` | Explicit `native_contract_not_implemented` response, status 501 | No successful native operation claimed |

The administrator key is generated into owner-restricted `admin.key`. It has no public read endpoint. Administrative requests use `X-Metallic-Admin`. Player requests use `Authorization: Bearer <session>`. Do not reuse a Rec Room password or import a third-party session. The optional local account-creation script prompts for a new password without placing it in command-line arguments.

## Storage and failure behavior

SQLite stores accounts, password hashes, hashed session tokens, expiry, profile revisions, and room metadata. WAL mode and FULL synchronous writes are enabled. Queries use parameters. Database schema version and integrity are checked at startup; an unexpected version fails without an automatic rewrite. A process lock prevents two hosts from owning the same data directory.

Passwords use ASP.NET's versioned password hasher with 600,000 iterations. Session tokens contain 32 random bytes; only their SHA-256 hashes are stored. Session revocation and expiry are checked on HTTP access, during WebSocket messages, and periodically while connected. Profile updates use revision checks so simultaneous writes do not silently overwrite one another.

An online SQLite backup runs every three hours while this service remains running. Eight timestamped backup files are retained. Each is checked and hashed before reporting success. Restore has been exercised in an isolated test directory. These local backups are **not off-machine, immutable, ransomware-proof, or a guarantee against all corruption**. An independent off-machine backup policy is still needed for a public deployment.

Live room membership and pose snapshots are intentionally transient. A server restart preserves accounts/rooms/sessions but creates a fresh live-room epoch. It does not pretend to restore full original-game physics or a multiplayer simulation.

Connection cleanup releases membership and capacity even when a session monitor fails with an unexpected storage exception. The failed socket is not treated as successful, and restoring storage allows the account to reconnect without restarting the host. See [the isolated storage-fault regression](../../docs/evidence/ROOM_CLEANUP_2026-10-28.md).

## Owned room protocol v1

The upgrade path is `/metallic/v1/rooms/{id}/socket`. The first message is a `snapshot`; subsequent events include `joined`, `pose`, `left`, and `error`. Events carry a server-side room epoch/revision. Actor identity is determined from the session and cannot be supplied in a pose message.

```json
{"type":"pose","sequence":1,"x":1.25,"y":2,"z":-3}
```

Every pose must explicitly supply type, sequence, x, y and z. Missing fields return invalid_json before changing room state; explicit zero coordinates remain valid. Sequences must increase per connection. Coordinates must be finite and within the declared bounds. The server checks room privacy, capacity, duplicate membership, frame size, and message rate. Slow peers have bounded output queues and are disconnected rather than allowing memory growth. Limits include 32 members per room, 128 active rooms, 256 active connections, a 4 KiB frame limit, and 60 messages/second per peer.

This is authenticated presence and pose replication, **not an authoritative Rec Room gameplay/physics server, cheat prevention system, or voice service**.

## Verification and launcher

```powershell
dotnet build Metallic.RC.slnx -c Release -p:RestoreLockedMode=true
dotnet run --project tests/Metallic.RC.Backend.Tests -c Release -- result-new.json
dotnet run --project src/Metallic.RC.Launcher -c Release -- probe-backend http://127.0.0.1:4187/
```

The launcher probe performs a real health/discovery handshake, checks the response through the compatibility library, and rejects addresses escaping the owned origin. It never starts the original game or stores a client discovery cache.

The backend integration suite uses real HTTP and independent WebSocket clients, tests access controls and negative requests, and restarts/restores real SQLite state. It does not require the private captured game inputs. Separately, `tools/rea/live_backend_probe.py` checks a live discovery body against the exact captured formatter instructions in an isolated CPU guest with explicitly modeled runtime/parser callbacks. Neither test is relabeled as original-game acceptance.

See `ops/backend/README.md` for local deployment and recovery controls, and `docs/evidence/OWNED_BACKEND_2026-10-21.md` for the measured checkpoint.
