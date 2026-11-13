# Metallic-RC — Staff Role Policy Kernel

**Implementation status:** server-side pure permission logic and unit tests committed. This is NOT a deployed staff API or a working in-game UI.

## Implemented today

- Verified-grant eligibility for the cosmetic **Volunteer Mod** and **Developer** titles.
- A staff member can switch their above-head title on/off independently of profile title visibility.
- Developer grants do not automatically gain Volunteer Mod moderation commands.
- Active global grants are required for globally verified badges; room-scoped assignments cannot impersonate global staff.
- Revoked or expired grants lose privileges and title eligibility.
- Preferences reject unexpected role/owner/admin fields, so a client cannot grant itself powers by toggling visibility.
- Action checks verify capability and global/room/staging scope.

Source: [role-policy.mjs](role-policy.mjs). Regression tests: [role-policy.test.mjs](tests/role-policy.test.mjs).

To run locally from repository root:

    node --test services/staff/tests/role-policy.test.mjs

## Correct production architecture

The role-policy module must be called by an **authenticated server** using grants loaded from the server-owned audited database. Never accept player-supplied roles as the grants argument. Authorization additionally needs case/target policies, session identity, MFA for high-impact actions, durable audit receipts and revocation event propagation.

Only a game renderer can display a real overhead avatar title. That is not available yet. Future implementation steps:
1. Add persistent staff grants, user preferences and role-revocation migrations.
2. Implement account-owned staff presence API and role assignment with audit trails.
3. Implement the overhead title in original desktop/Quest avatar rigs with visibility and occlusion/LOD tests.
4. Connect in-game, web and Discord mod actions to one trusted server-side command handler.
5. Verify permissions, private session behavior, staff impersonation defense and responsive VR UI.

Additional specification: [Moderation, Discord and Staff](../../docs/MODERATION_DISCORD_STAFF.md), task [#43](https://github.com/oncoursemail/Metallic-RC/issues/43) and [#44](https://github.com/oncoursemail/Metallic-RC/issues/44).

## Example title preference

A supported staff member chooses preferredTitle = volunteer_mod, showOverheadTitle = true, showProfileTitle = false. The server checks the staff grant and returns the display state; changing it never changes the role.

**Not yet implemented:** staff grants persistence, auth service, live Discord moderation commands, game client rendering, Quest build or public release.
