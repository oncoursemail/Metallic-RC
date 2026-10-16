# Metallic Studios — Staff, Volunteer Moderator, Developer Titles, In-Game Tools & Discord Operations

**Research baseline:** 2026-10-15 · **Status:** Design specification, not implemented.
**Goal:** a coordinated, auditable operations system available *in game*, through a secure web console, and through carefully permissioned Discord commands. Rec Room-style Volunteer Moderator / Developer identity display is a requested reference, **not** a claim that Metallic has recreated original code.

## 1. Three entry points, one authority

\`\`\`mermaid
flowchart LR
  GAME["In-game staff panel / VR watch"] --> AUTH["Moderation API / authorization"]
  WEB["Web staff command center"] --> AUTH
  BOT["Discord bot (allowlisted commands)"] --> AUTH
  AUTH --> RBAC["Scoped role and capability policy"]
  AUTH --> CASE["Case management + sanctions"]
  AUTH --> ROOM["Room service and session commands"]
  AUTH --> AUDIT["Append-only privileged action audit"]
  ROOM --> PUSH["Server-side effects + client UI notifications"]
\`\`\`

Never implement staff commands as independent, uncoordinated sources of truth. A Discord role alone is not permission to ban inside the game; only the Metallic role/capability service can authorize the action. User-visible badges/titles are *cosmetic representation* of an authenticated role, not proof of staff authority.

## 2. Roles, privileges, and above-head titles

| Role | Visible nametag text | Default title visibility | Granted abilities (sample) |
| --- | --- | --- | --- |
| Player | none | not applicable | Own account, report, block/mute |
| Room owner / room moderator | optional room-specific tag | off by default outside room | Manage the room only; never global ban by virtue of room ownership |
| **Volunteer Moderator (VMod)** | **Volunteer Mod** | user-controlled on/off | Assigned case review, warnings, scoped temporary mute / room removal with reasons |
| **Developer** | **Developer** | user-controlled on/off | Development tools in test environments; no automatic production moderation powers |
| Staff Moderator | **Moderator** | user-controlled on/off | Casework and appropriate sanctions under assigned scope |
| Lead Moderator / Trust & Safety | **Lead Moderator** | on unless policy changes | Review sanctions/appeals and assist volunteer mods |
| Infrastructure Operator | **Operations** if explicitly enabled | off | Fleet controls, restart and incident access, **not** access to all private conversations by default |
| Owner / Superadmin | **Metallic Staff** or configurable branded title | user-controlled | Global configuration and controlled emergency response |

**Important separation:** \`role\`, \`capabilities\`, \`show_public_title\`, \`display_title_variant\`, and \`room_permission\` are different fields. Turning off a title **does not remove powers** and turning it on cannot grant powers. A developer can also be a VMod; title priority requires an explicit policy rather than rendering two overlapping tags.

### The desired title behavior

- Approved VMods get a real Metallic-granted staff role and a **Volunteer Mod** title option in personal Settings → Identity & Presence → Staff Title.
- Approved developers get a **Developer** title option there.
- Toggle **Show title above my avatar** on/off and **Show on profile** independently (if policy permits). Local immediate preview; server saves authoritative preference; other players' displays update via small reliable event.
- Render title in a compact nametag above the real avatar head, never directly over the face, eyes or usernames. Use stable world-space spacing; add per-platform distance scaling and LOD; display near-head readable title only when the user is within reasonable viewing distance.
- For VMods: a tiny shield marker and text; for Developers: a tiny verified tool/star marker and text. Color distinguishes function without relying on color alone.
- Keep title hidden in screenshots/streamer privacy mode if the owner disables it.
- Users must not be able to submit \`isVMod: true\` client patches. Permissions come from audited server role assignment.
- A spoofed badge texture/name is not proof of authority. Profile verification state comes from a signed, server-authenticated role response.
- Show role revocations in connected clients quickly and revoke active staff commands/session scopes; maintain a dedicated audit trail even if badge hidden.
- VR test full headset text legibility, title occlusion behind walls and frame-time budget with 32+ visible nametags.

### Suggested data structure

\`\`\`sql
CREATE TABLE staff_grants (
  id UUID PRIMARY KEY,
  player_id UUID NOT NULL,
  role TEXT NOT NULL,
  scope_type TEXT NOT NULL,
  scope_id UUID,
  granted_by UUID NOT NULL,
  granted_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ,
  reason TEXT NOT NULL
);

CREATE TABLE player_staff_presence (
  player_id UUID PRIMARY KEY,
  show_overhead_title BOOLEAN NOT NULL DEFAULT FALSE,
  show_profile_title BOOLEAN NOT NULL DEFAULT FALSE,
  preferred_title TEXT,
  updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE staff_action_audit (
  id UUID PRIMARY KEY,
  actor_player_id UUID NOT NULL,
  target_player_id UUID,
  action TEXT NOT NULL,
  scope_type TEXT NOT NULL,
  scope_id UUID,
  case_id UUID,
  reason_code TEXT NOT NULL,
  request_id TEXT NOT NULL UNIQUE,
  occurred_at TIMESTAMPTZ NOT NULL
);
\`\`\`

These are **schema examples** and require real indexes, foreign keys, partition/retention choices and migrations when implemented.

### Server authority for title settings

\`\`\`http
GET   /v1/me/staff-presence
PATCH /v1/me/staff-presence    {"showOverheadTitle": true, "showProfileTitle": false, "preferredTitle": "volunteer_mod"}
GET   /v1/players/{id}/verified-public-roles
POST  /v1/admin/staff-grants
DELETE /v1/admin/staff-grants/{grant_id}
\`\`\`

**PATCH algorithm:** authenticate -> server loads active unexpired grant(s) -> ensure selected title is allowed for grant -> update one preference field -> emit reliable \`PublicRoleVisibilityChanged\` -> update session on next authoritative state sync -> return resolved display state. Never cache privilege authorization indefinitely or use client-supplied grant data.

## 3. In-game moderation & staff control panel

### Desktop UI

**Entry:** account menu → Staff Tools. Only visible if the server returns permitted capabilities; hiding UI alone is not the access check.

**Navigation:** 
- **Queue** — assigned reports, age of case, severity, actionable location, last moderator activity, duplicate/related reports.
- **Players** — scoped player search by ID/nickname (avoid weak partial fuzzy matches for sanctions), current visible sessions and recent case history.
- **Rooms** — room occupancy, safe pause/close controls, creator ownership, server region, load, circuit throttling indicators.
- **Actions** — warn, silence voice, text mute, temporary kick, room-instance block, timed suspension, escalate, retract under policy.
- **Evidence** — authorized and time-limited report attachments, action context, permitted voice samples where appropriately captured, redactions.
- **Appeals** — case state, decision, second-reviewer assignment, evidence exclusion.
- **System health** — player counts (humans vs synthetic), alert list, room server CPU/memory, p95 tick time, dropped packets, SFU state.
- **Broadcasts** — preview announcement; permission check and dry run, then targeted audience delivery.

**Action dialog:** exact target identity, scope, reason and expiration must be visible; high-impact action requires confirmation and second approver based on policy. Actions must be idempotent, attributable and revocable when appropriate.

### VR-specific staff UI

- Wrist/hand-origin bounded panel with legible tab labels; no enormous permanent overlay near the eyes.
- Large touch/grab targets, text scale, controller/hand input parity, scroll & back behavior; minimize typing by allowing predefined reason choices and voice-to-text only when enabled.
- Moderator teleport/spectate only through scoped, audited commands and user safety safeguards; show operator identity and purpose in logs.
- Critical production actions should be deliberately harder than accidental trigger. Role-assignment and permanent global sanctions belong in the web console with step-up MFA.

## 4. Web operations command center

**Screens:** real-time overview, role manager, active room table, fleet regions, queue/appeals, content review, economy/fraud exceptions, audit events, incident log, creator verification, Discord integrations, release channels, synthetic lab jobs.

**Safeguards:** OIDC/passkey/MFA; scoped policy per action; session expiration; CSRF, CSP, safe audit logs, step-up auth for dangerous actions, immutable request IDs; do not place back-end admin credentials into frontend bundles.

**Operational metrics:** live real human player counts (with stale-age), total sessions, queued joins, active room density, p95 server tick, percent dropped packets, crash-free hours, voice SFU load, in-progress bot load labeled synthetic. Every chart labels interval and timestamp so test data can never be mistaken for live production player counts.

**Permission examples:**

| Action | VMod | Developer | Lead Mod | Ops | Owner |
| --- | --- | --- | --- | --- | --- |
| Read assigned reports | assigned | no | yes | no | yes |
| Warn / room-scoped mute | scoped | no | yes | no | yes |
| Global permanent ban | no | no | dual approval | no | dual approval |
| Assign/revoke VMod grant | no | no | propose | no | approve |
| Turn on own staff title | yes | yes | yes | yes | yes |
| View game fleet health | limited | dev/test | limited | yes | yes |
| Start staging load test | no | approved environment | no | yes | yes |
| Restart a room server | no | test only | no | yes | yes |
| Edit economic balances | no | no | no | no | highly controlled audited recovery |
| Modify staff role policy | no | no | no | no | dual-control change |

## 5. Discord bot: product plan

The bot is an operations and community interface, not a replacement for the game backend.

**Recommended implementation:** discord.js v14+ using only necessary Gateway intents and slash commands, plus an internal service-to-service client with scoped token. Documentation: [discord.js ClientUser.setPresence](https://discord.js.org/docs/packages/discord.js/main/ClientUser%3AClass) and [Discord application commands](https://discord.com/developers/docs/interactions/application-commands).

**Public features**
- \`/help\` — grouped commands for new players/creators/testers/staff.
- \`/status\` — production status, maintenance windows, last health sample.
- \`/players\` — total **real authenticated active players** from authoritative session counters (not Discord member count, not bot sessions).
- \`/servers\` — region health, queue and free room capacity, no private endpoints.
- \`/rooms search <tags>\` — approved public room results.
- \`/room <id>\` — public room info and safe join link.
- \`/link\` — start short-lived website OAuth linking (not bot-DM passwords).
- \`/profile\` — player's public privacy-allowed profile.
- \`/events\`, \`/patchnotes\`, \`/roadmap\`, \`/feedback\`, \`/bug\`, \`/support\`, \`/appeal\`.
- \`/creator\`, \`/publish-guide\`, \`/assets\` and creator support resources.
- Optional verification and opt-in community announcements, no DM spam.

**Staff commands** (server authorized, admin action receipts, ephemeral sensitive replies):
- \`/mod queue\`, \`/mod case <case_id>\`, \`/mod warn <player_id> <reason>\`, \`/mod mute <player_id> <duration>\`, \`/mod escalate <case_id>\`.
- \`/staff roster\`, \`/staff role-request\` — never direct role grant by a random Discord server admin.
- \`/ops status\`, \`/ops incident\`, \`/ops maintenance\`, \`/ops region-health\`, \`/ops load-test-status\` (read-only).
- \`/announce draft\` and \`/announce publish\` — two-step permissioned announcement workflow.
- \`/audit receipt <id>\` — redacted audit receipt visible only to authorized staff.

**Bot activity/status:** show e.g. \`Playing Metallic · 427 online\` when an authoritative, fresh number is available; update only when the count meaningfully changes or periodically at a **conservative interval** (such as 60–120 s), not every player join. If stale, switch activity to \`Maintenance / status unavailable\`, not a guessed number or \`0 online\`. The bot's \`setPresence\` activity refers to **bot's own presence**; a player's individual Discord Rich Presence is separate.

**Presence aggregation contract** (proposed):
\`\`\`json
{
  "schema": 1,
  "environment": "production",
  "observedAt": "2026-10-15T23:00:00Z",
  "realAuthenticatedActivePlayers": 427,
  "syntheticTestPlayers": 0,
  "activeRoomSessions": 83,
  "ttlSeconds": 60,
  "source": "server-heartbeat-aggregator"
}
\`\`\`
Numbers here are **illustrative**, NOT measured Metallic metrics. The real online count should use room-server heartbeats (with TTL), dedupe account/session IDs and exclude artificial players. A player with multiple devices should not count more than policy permits. Exclude hidden/private room names from public commands.

**Resilience:** Discord outage cannot affect the game, backoff and caching for status fetch, token encryption/rotation, slash command validation at the backend, rate limit user commands, use explicit command permission scopes.

## 6. Player Discord Rich Presence

Allow opt-in **user-specific** Discord activity where the supported desktop/VR SDK can set presence:
- **Show activity** (off/on)
- **Show current public room** (off/on, dependent on activity)
- **Allow friends to join via Discord** (off/on, only with verified room-level invites)
- **Streamer / invisible mode** (hides room and session details)

If the player enters a private/unlisted/age-restricted/moderation-sensitive room, presence should fall back to generic Metallic activity or nothing. Non-PC platforms may have SDK limitations, so the UI must not promise impossible feature parity.

Docs: [Discord Social SDK Unity guide](https://docs.discord.com/developers/discord-social-sdk/getting-started/using-unity) (supported runtime integrations must be tested).

## 7. Source and product-context references

- Historical Rec Room community request for role tags similar to VMods: [Rec Room creator forum, 2024](https://forum.rec.net/t/add-tags-under-usernames-for-workshop-hosts-and-teachers/1806) — **community description of reference behavior, not a current platform implementation specification**.
- Discord [presence data](https://discord.js.org/docs/packages/discord.js/14.18.0/PresenceData%3AInterface) and [setPresence](https://discord.js.org/docs/packages/discord.js/main/ClientUser%3AClass).
- Discord [Unity Social SDK](https://docs.discord.com/developers/discord-social-sdk/getting-started/using-unity).
- General [OWASP authorization guidance](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html), relevant to shared server-side permissions.
- Meta [spatial typography guidance](https://developers.meta.com/vr/design/styles_typography/), relevant for overhead nameplate/text scale in VR.

## 8. Acceptance matrix

- [ ] Newly granted VMod receives only scoped capabilities and **Volunteer Mod** title option.
- [ ] Newly granted Developer receives **Developer** title option but no auto moderation powers.
- [ ] Overhead title can be turned on/off **without changing access privileges**.
- [ ] Other clients correctly see the change; hidden titles do not leak via nameplate or profile if hidden.
- [ ] Role revocation disables commands immediately (or within a documented short TTL) and hides title.
- [ ] Permission tests attempt each command as every role, including expired roles.
- [ ] In-game, web and Discord actions all hit the same authorization and audit logic.
- [ ] Staff actions require reason, scope, case link as applicable and create durable receipts.
- [ ] Discord \`/players\` and bot activity exclude synthetic accounts and stale data.
- [ ] Discord user presence respects user and room privacy; changes propagate after room transitions.
- [ ] Real-headset profile shows readable labels and never overlays title text with the face/other HUD.
- [ ] Security/abuse review, sanction appeals and protected data retention are tested before public rollout.

**No bots, role grants, production moderators, accounts, status feed or real-time panels are deployed by this document.**
