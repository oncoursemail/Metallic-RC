# metallic-rc-server

The Metallic-RC backend in Rust. One HTTP server answers as every RecNet service the 2026 client (Steam build
23191908) gets from its name server.

```
cargo run --release -- --bind 127.0.0.1:4200 --data-dir data
cargo test
```

| Service | Path on this server | What is served |
| --- | --- | --- |
| Name server | `/` or `/nameserver?buildId=…` | 32 fields: `RecNetStatus` (null = up) and 31 service URLs, all `<public-url>/<service lower case>` |
| Auth | `/auth/connect/token` | form POST; Steam `platform_id`, `device_id` or `username` login, `refresh_token` grant; HS256 tokens |
| Accounts | `/accounts/account/me`, `/{id}`, `/bulk?id=` | account records |
| API | `/api/config/v2`, settings, avatar, inventory, relationships, messages, events, objectives, progression | config plus empty lists |
| Rooms | `/rooms/rooms/bulk?name=`, `/rooms/rooms/{id}` | built-in DormRoom and RecCenter |
| Matchmaking | `/matchmaking/player/login`, `heartbeat`, `goto/room/{name}` | presence; a private dorm instance per player |
| Notifications | `/notifications/hub/v1` (+ `/negotiate`) | SignalR JSON hub: handshake, pings, invocations answered and logged |

Everything else gets a 404 and a line in `<data-dir>/unknown.jsonl` (method, path, query, start of the body): after a
client run, that file is the list of what to build next.

Data: `<data-dir>/accounts.json` and `<data-dir>/signing.key` (the token key; keep it private, never commit it).

Not done yet:
- Photon (the realtime server where players meet).
- Room scene ids for the 2026 client.
- Steam ticket checks: the server trusts the platform id it is sent, so it is only for a private test server.
- The client's exact 2026 response shapes beyond the name server and login.
