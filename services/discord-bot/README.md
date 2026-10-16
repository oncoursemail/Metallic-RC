# Metallic Discord Bot — Working Foundation (Not Connected to Production)

The code registers Discord guild-scoped slash commands and can run as a Node.js bot once the owner provides **their own** Discord application token. The public Metallic game backend, authentic heartbeat counts and production moderation API **do not exist yet** in this repository; the bot returns **unavailable**, never a fabricated online count.

## Commands currently coded

- \`/help\` — command guide
- \`/players\` — server-reported real human players, when a trusted production heartbeat endpoint becomes available
- \`/status\` — observed service-status feed availability, not invented uptime
- \`/roadmap\` — links the GitHub roadmap

The bot updates its own Discord Playing activity conservatively (every 120s), showing \`Metallic · N online\` when it has a fresh, validated **production** aggregate. The validator excludes synthetic load-test actors and rejects staging data. Public online count comes from authoritative game-server heartbeats, **not** Discord server membership or a fake-client estimate.

Player-specific Discord Rich Presence is **another future game client feature**; this bot does not override Discord users' personal statuses.

## Status-feed client (\`src/feed.mjs\`, added for issue #38)

\`StatusFeed\` is the single source of truth both for the bot's presence label and for \`/players\`/\`/status\`, so those two surfaces **cannot** show different counts:

- **Shared snapshot:** presence refresh and slash commands read the same cached snapshot; concurrent callers share one in-flight HTTP request (no thundering herd against the status API).
- **Freshness:** each snapshot carries the feed's own \`ttlSeconds\`; once older than that TTL the count becomes *unavailable* instead of being displayed stale.
- **Safe fallback:** on a network/HTTP failure the last good snapshot is served **only while it is still inside its TTL**; after that the bot says unavailable. Genuine outages never render as \`0 online\`.
- **Auth/transport:** optional \`Bearer\` service-to-service token, HTTPS-only URLs (loopback HTTP allowed for local development), 4 s timeout, redirects refused, non-2xx treated as unavailable.
- **Diagnostics:** failures reach an \`onError\` hook that logs only the error name — never tokens, full URLs or response bodies.

## Setup (explicit owner action required)

1. Create an application/bot in the [Discord Developer Portal](https://discord.com/developers/applications), authorize it in a development guild with the \`bot\` and \`applications.commands\` scopes, and only needed permissions.
2. Install supported Node.js (22+), go to this folder, run \`npm install\` then \`npm test\`.
3. Provide **environment variables**, not checked-in secrets:

\`\`\`text
METALLIC_DISCORD_TOKEN=<secret_bot_token>
METALLIC_DISCORD_APP_ID=<application_id>
METALLIC_DISCORD_GUILD_ID=<authorized_test_guild_id>
METALLIC_ROADMAP_URL=https://github.com/oncoursemail/Metallic/blob/main/docs/ROADMAP.md
METALLIC_STATUS_URL=<future_production_status_api_https_url>
METALLIC_STATUS_BEARER=<optional_private_server_to_server_token>
\`\`\`

\`METALLIC_STATUS_URL\` may be left unset until the backend is built. Never set it to a simulated/test system and pretend that response represents real production players.

4. Run \`npm run register\` once to add guild-scoped slash commands, then \`npm start\`. The bot connects only after explicit owner configuration. It has **not** been installed or logged in by ChatGPT.

## Planned trusted status API contract

\`\`\`json
{
  "environment": "production",
  "source": "server-heartbeat-aggregator",
  "realAuthenticatedActivePlayers": 42,
  "syntheticTestPlayers": 0,
  "activeRoomSessions": 3,
  "observedAt": "2026-10-15T20:00:00Z",
  "ttlSeconds": 60
}
\`\`\`

This is a **schema example**, not a real count. The API should use authenticated HTTPS and signed/stable upstream session heartbeats, dedupe users with multiple connections, exclude load-test namespaces and enforce staleness. The bot's string validation is not itself cryptographic attestation; server authenticity comes from the trusted authenticated transport and secure service-to-service access.

## Extending later

The full spec is in [Moderation, Discord and Staff](../../docs/MODERATION_DISCORD_STAFF.md). Next stages: \`/servers\`, \`/rooms\`, \`/events\`, \`/appeal\`, \`/creator\`, and staff commands **after** the central Metallic RBAC/moderation service supports scoped authorization, case IDs, reasons and append-only audit receipts. A Discord guild role is not enough to authorize an in-game sanction.

## Quality and privacy

- No privileged message-content/member intent required.
- No access tokens in the repository.
- No public exposure of private room/player identity from the status API.
- Bot activity uses verified source/TTL checks; missing data reports unavailable instead of zero.
- The bot does **not** automatically DM users or broadcast internal moderation data.
- Run \`npm test\` to validate freshness, URL policy and synthetic-count handling. \`tests/feed.test.mjs\` runs a loopback HTTP server to prove bearer-token delivery, shared presence//players snapshots, TTL expiry, outage fallback and request dedupe end to end.
- The \`Discord status feed integration gate\` GitHub Actions workflow syntax-checks every entrypoint and runs the whole bot suite on each change (no Discord secrets needed in CI).

## Limitations, supported environments and risks

**Supported and tested:**

- Node.js 22+ on the machines that run the bot (CI verifies Linux; the code is dependency-light Node built-ins plus discord.js 14, so macOS/Windows Node 22+ should work but are not CI-proven).
- One Discord guild configured via \`METALLIC_DISCORD_GUILD_ID\`; commands outside it get an ephemeral refusal.
- The schema-exact status contract below; anything else is treated as unavailable.

**Limitations:**

- No production status API exists in this repository yet — until the owner points \`METALLIC_STATUS_URL\` at a real authenticated HTTPS feed, all counts show unavailable by design.
- String/schema validation is not cryptographic attestation of the upstream aggregator; security rests on the authenticated HTTPS channel and the server-to-server token.
- The status validator trusts \`environment: "production"\` and \`source: "server-heartbeat-aggregator"\` declarations; a compromised upstream could lie. Deduplication of multi-device players and load-test exclusion are the aggregator's responsibility.
- Presence updates on a 120 s timer, so Discord's displayed count can lag the latest snapshot by up to that interval; \`/players\` reads the same snapshot and is at most one \`minRefreshMs\` (60 s) behind the feed.
- Rate limiting of users invoking the commands repeatedly is not yet implemented.

**Risks and mitigations:**

- *Token leak:* tokens are env-only, never logged, and the URL validator rejects embedded credentials.
- *Stale data presented as live:* TTL is enforced at read time, not just fetch time, so cached counts expire.
- *Upstream outage:* failures degrade to the last still-fresh snapshot, then to unavailable; a real \`0\` is only shown when the fresh production payload reports zero humans.
- *Discord outage:* cannot affect the game; the bot only fails to post presence.
- *Scope creep into moderation:* staff/moderation commands stay disabled until Metallic's central RBAC/moderation API exists ([spec](../../docs/MODERATION_DISCORD_STAFF.md)).

[Discord slash command docs](https://docs.discord.com/developers/docs/interactions/slash-commands) · [discord.js reference](https://discord.js.org/).
