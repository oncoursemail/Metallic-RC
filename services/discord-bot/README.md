# Metallic Discord Bot — Working Foundation (Not Connected to Production)

The code registers Discord guild-scoped slash commands and can run as a Node.js bot once the owner provides **their own** Discord application token. The public Metallic game backend, authentic heartbeat counts and production moderation API **do not exist yet** in this repository; the bot returns **unavailable**, never a fabricated online count.

## Commands currently coded

- \`/help\` — command guide
- \`/players\` — server-reported real human players, when a trusted production heartbeat endpoint becomes available
- \`/status\` — observed service-status feed availability, not invented uptime
- \`/roadmap\` — links the GitHub roadmap

The bot updates its own Discord Playing activity conservatively (every 120s), showing \`Metallic · N online\` when it has a fresh, validated **production** aggregate. The validator excludes synthetic load-test actors and rejects staging data. Public online count comes from authoritative game-server heartbeats, **not** Discord server membership or a fake-client estimate.

Player-specific Discord Rich Presence is **another future game client feature**; this bot does not override Discord users' personal statuses.

## Setup (explicit owner action required)

1. Create an application/bot in the [Discord Developer Portal](https://discord.com/developers/applications), authorize it in a development guild with the \`bot\` and \`applications.commands\` scopes, and only needed permissions.
2. Install supported Node.js (22+), go to this folder, run \`npm install\` then \`npm test\`.
3. Provide **environment variables**, not checked-in secrets:

\`\`\`text
METALLIC_DISCORD_TOKEN=<secret_bot_token>
METALLIC_DISCORD_APP_ID=<application_id>
METALLIC_DISCORD_GUILD_ID=<authorized_test_guild_id>
METALLIC_ROADMAP_URL=https://github.com/oncoursemail/Metallic-RC/blob/main/docs/ROADMAP.md
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

A local implementation of the producing service (authenticated heartbeat ingest, de-duplication and synthetic-actor separation) lives in [services/player-count](../player-count/README.md). It is not deployed and emits no production count.

## Extending later

The full spec is in [Moderation, Discord and Staff](../../docs/MODERATION_DISCORD_STAFF.md). Next stages: \`/servers\`, \`/rooms\`, \`/events\`, \`/appeal\`, \`/creator\`, and staff commands **after** the central Metallic RBAC/moderation service supports scoped authorization, case IDs, reasons and append-only audit receipts. A Discord guild role is not enough to authorize an in-game sanction.

## Quality and privacy

- No privileged message-content/member intent required.
- No access tokens in the repository.
- No public exposure of private room/player identity from the status API.
- Bot activity uses verified source/TTL checks; missing data reports unavailable instead of zero.
- The bot does **not** automatically DM users or broadcast internal moderation data.
- Run \`npm test\` to validate freshness, URL policy and synthetic-count handling.

[Discord slash command docs](https://docs.discord.com/developers/docs/interactions/slash-commands) · [discord.js reference](https://discord.js.org/).
