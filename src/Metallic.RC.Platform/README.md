# Metallic-RC local authoring service

This .NET 10 service persists local room drafts and validates and previews the original Metallic CV3 graph format. The current runtime contains eleven implemented nodes. The capability response explicitly reports that a compiled Rec Room client adapter, Rec Room protocol, multiplayer, voice and account migration are unavailable.

## Run and verify

```sh
dotnet run --project src/Metallic.RC.Platform -- --port 4186 --data-dir ./local-metallic-data
dotnet run --project tests/Metallic.RC.Platform.Tests -c Release
```

The service binds `127.0.0.1` only. The default port is `4186`; command-line ports must be between `1024` and `65535`. Without `--data-dir`, state goes in the current user's local application-data directory under `Metallic-RC/platform`. A data directory is dedicated to one running instance. Its directory and session-file permissions are restricted to the current operating-system user. The launcher supplies its own chosen data directory.

The acceptance executable uses actual temporary loopback HTTP listeners and temporary storage. It needs no test framework or external package. It checks authentication and browser-origin boundaries, content limits including chunked requests, concurrent ETag updates, restart persistence, token rotation, corrupt storage handling, graph typing, cycle errors, stable hashes and bounded preview execution.

## Local session

The same-origin frontend requests `GET /api/session` with the header `X-Metallic-Client: desktop`. The response is `{ "token": "...", "apiVersion": "1" }`. Keep the token in memory and send `X-Metallic-Token` on every `POST`, `PUT` and `DELETE`, including validation and preview requests. The service also writes the current token, port and process ID to `local-session.json` in its private state directory. It never prints the token.

The service checks the exact loopback host and bound port. If supplied, `Origin` must equal the service origin and `Sec-Fetch-Site` must be `same-origin` or `none`. CORS preflight is not enabled. Responses disable caching, disallow framing and restrict scripts to the same origin. This is a local desktop authoring boundary; it is not a public account or remote-access service.

## HTTP contract

All request and response properties use camelCase. JSON request fields are case-sensitive. Unknown or duplicate properties are rejected. The body limit is 262,144 bytes.

| Method and path | Body / condition | Result |
| --- | --- | --- |
| `GET /healthz` | None | Service health and `scope: "local-authoring"` |
| `GET /api/session` | `X-Metallic-Client: desktop` | Current local token and API version |
| `GET /api/v1/capabilities` | None | Implemented features, unavailable integrations, limits and hashed workspace identity |
| `GET /api/v1/cv3/catalog` | None | Supported node types, ports and configuration keys |
| `POST /api/v1/cv3/validate` | Graph object | Validation issues or a canonical SHA256 program hash |
| `POST /api/v1/cv3/preview` | `{ graph, maxSteps?, initialState? }` | Bounded execution result, logs, state and issues |
| `GET /api/v1/rooms` | None | `{ rooms: [...summaries], storage: "local" }` |
| `POST /api/v1/rooms` | `{ name, description?, graph? }` | `201`, full draft, `Location`, `ETag: "r1"` |
| `GET /api/v1/rooms/{id}` | None | Full draft and current ETag |
| `PUT /api/v1/rooms/{id}` | `{ name, description?, graph }`, `If-Match` | Updated full draft and incremented ETag |
| `DELETE /api/v1/rooms/{id}` | `If-Match` | `204` |

`workspaceId` is lowercase SHA256 of the UTF-8 full data-directory path after trimming its trailing directory separator and, on Windows, converting it to uppercase with invariant culture. It identifies the configured workspace without returning its filesystem path. The launcher compares this value before attaching to an already-running service.

Full drafts contain `id`, `name`, `description`, `revision`, `createdAt`, `updatedAt` and `graph`. Summaries contain the same fields except `graph`. Room IDs are generated GUIDs in `N` format; they cannot address arbitrary paths. Names have 1–120 characters and descriptions have at most 4,000. The store supports at most 500 drafts. It writes a flushed temporary file and atomically replaces the prior draft. The store is a local filesystem implementation, with no distributed replication or room-hosting behavior.

Missing `If-Match` produces `428`; an out-of-date revision produces `412`; an invalid strong ETag produces `400`. Error responses are `{ "error": { "code": "...", "message": "..." } }`. Storage failures produce a redacted `503`, never a successful empty draft or reset. Graph validation failures return `200` with `valid: false` or `succeeded: false` so the editor can display node-level issues. Unfinished graphs can be saved within the structural limits and cannot be previewed until valid.

## Graph format

```json
{
  "schemaVersion": 1,
  "entryNodeId": "start",
  "nodes": [
    { "id": "start", "kind": "event.start", "config": {} },
    { "id": "message", "kind": "constant.string", "config": { "value": "Hello Metallic" } },
    { "id": "log", "kind": "debug.log", "config": {} }
  ],
  "edges": [
    { "fromNode": "start", "fromPort": "next", "toNode": "log", "toPort": "exec" },
    { "fromNode": "message", "fromPort": "value", "toNode": "log", "toPort": "value" }
  ]
}
```

| Node kind | Inputs | Outputs | Configuration |
| --- | --- | --- | --- |
| `event.start` | None | `next: exec` | None |
| `constant.number` | None | `value: number` | Finite numeric `value` |
| `constant.boolean` | None | `value: boolean` | Boolean `value` |
| `constant.string` | None | `value: string` | Text `value`, up to 4,096 UTF-16 characters |
| `number.add` | `a`, `b`: number | `value: number` | None |
| `number.multiply` | `a`, `b`: number | `value: number` | None |
| `number.lessThan` | `a`, `b`: number | `value: boolean` | None |
| `flow.branch` | `exec`, `condition: boolean` | `true`, `false`: exec | None |
| `debug.log` | `exec`, `value`: scalar | `next: exec` | None |
| `state.setNumber` | `exec`, `value: number` | `next: exec` | State `key` |
| `state.getNumber` | None | `value: number` | State `key`; missing values read as zero |

Each graph has exactly one `event.start` entry and at most 256 nodes and 1,024 edges. Node IDs are unique, bounded identifiers. Every declared input is required. Data inputs accept one connection; execution inputs can merge; each execution output selects at most one next node. Validation rejects unsupported node kinds, unknown configuration, missing ports, incompatible types, duplicate edges, missing inputs and all data or execution cycles. This version supports acyclic control flow rather than loop nodes.

Validation returns `{ valid, issues, programHash, nodeCount, edgeCount }`. Issues have `code`, `message`, optional `nodeId` and optional `port`. Valid programs receive a lowercase SHA256 hash of canonical graph data: fixed property order, sorted node IDs and edges, sorted configuration keys and normalized finite numeric representations. Editor array ordering does not affect the hash.

Preview returns `{ succeeded, programHash, steps, logs, state, issues }`. Each log has `nodeId`, `valueType`, scalar `value` and formatted `text`. The default budget is 512 steps; the maximum is 4,096. Every control node and every data evaluation consumes one step, and data dependency depth is limited to 64. Initial state is a dictionary of at most 128 finite numeric values; execution state is limited to 256 keys. Arithmetic overflow is a node-level error. On a preview stop, returned logs and state reflect completed operations only. Preview state is temporary; it does not alter the stored room draft or a game client.

The preview has no clock, random source, filesystem, networking or arbitrary code execution. This graph model is an original Metallic authoring format and is not an implementation of, or an injection into, Rec Room's creator runtime.
