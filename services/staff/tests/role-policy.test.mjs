import test from "node:test";
import assert from "node:assert/strict";
import {
  ROLES, TITLES, resolvePublicStaffTitle, canPerformStaffAction, validateTitlePreference,
} from "../role-policy.mjs";

const NOW = Date.parse("2026-10-15T22:00:00Z");
const grant = (role, extras = {}) => ({
  role, scopeType: "global", scopeId: null, revokedAt: null,
  expiresAt: null, ...extras,
});
const show = (preferredTitle) => ({
  preferredTitle, showOverheadTitle: true, showProfileTitle: true,
});

test("VMod has independent visible Volunteer Mod title toggles", () => {
  const grants = [grant(ROLES.VOLUNTEER_MOD)];
  const visible = resolvePublicStaffTitle(grants, show(TITLES.VOLUNTEER_MOD), NOW);
  assert.equal(visible.titleText, "Volunteer Mod");
  assert.equal(visible.overhead, true);
  assert.equal(visible.profile, true);
  const hidden = resolvePublicStaffTitle(grants, { ...show(TITLES.VOLUNTEER_MOD), showOverheadTitle: false }, NOW);
  assert.equal(hidden.overhead, false);
  assert.equal(hidden.profile, true);
  assert.equal(canPerformStaffAction(grants, "moderation.player.warn", {type:"global"}, NOW), true);
});

test("Developer title does not grant volunteer moderation", () => {
  const grants = [grant(ROLES.DEVELOPER)];
  assert.equal(resolvePublicStaffTitle(grants, show(TITLES.DEVELOPER), NOW).titleText, "Developer");
  assert.equal(canPerformStaffAction(grants, "development.staging.test", {type:"staging", id:"lab-1"}, NOW), true);
  assert.equal(canPerformStaffAction(grants, "moderation.player.warn", {type:"global"}, NOW), false);
  assert.equal(canPerformStaffAction(grants, "moderation.role.grant", {type:"global"}, NOW), false);
});

test("No-role and wrong-role players cannot forge an overhead staff title", () => {
  assert.equal(resolvePublicStaffTitle([], show(TITLES.VOLUNTEER_MOD), NOW).titleText, null);
  assert.equal(resolvePublicStaffTitle([grant(ROLES.DEVELOPER)], show(TITLES.VOLUNTEER_MOD), NOW).titleText, null);
  assert.throws(() => validateTitlePreference([], show(TITLES.DEVELOPER), NOW));
  assert.throws(() => validateTitlePreference([grant(ROLES.DEVELOPER)], {
    ...show(TITLES.DEVELOPER), role: "owner",
  }, NOW));
});

test("Expired or revoked roles immediately lose title and permissions", () => {
  for (const data of [{ revokedAt: "2026-10-15T21:00:00Z" }, { expiresAt: "2026-10-15T21:59:59Z" }]) {
    const grants = [grant(ROLES.VOLUNTEER_MOD, data)];
    assert.equal(resolvePublicStaffTitle(grants, show(TITLES.VOLUNTEER_MOD), NOW).titleText, null);
    assert.equal(canPerformStaffAction(grants, "moderation.player.warn", {type:"global"}, NOW), false);
  }
});

test("Room-scoped grants do not authorize global sanctions", () => {
  const g = [grant(ROLES.VOLUNTEER_MOD, {scopeType:"room",scopeId:"room-1"})];
  assert.equal(resolvePublicStaffTitle(g, show(TITLES.VOLUNTEER_MOD), NOW).titleText, null);
  assert.equal(canPerformStaffAction(g, "moderation.room.mute", {type:"room",id:"room-1"}, NOW), true);
  assert.equal(canPerformStaffAction(g, "moderation.room.mute", {type:"room",id:"room-2"}, NOW), false);
  assert.equal(canPerformStaffAction(g, "moderation.player.warn", {type:"global"}, NOW), false);
});

test("Owner can grant staff role only with backend global auth and cannot bypass scope", () => {
  assert.equal(canPerformStaffAction([grant(ROLES.OWNER)], "moderation.role.grant", {type:"global"}, NOW), true);
  assert.equal(canPerformStaffAction([grant(ROLES.OWNER, {scopeType:"staging",scopeId:"lab"})], "moderation.role.grant", {type:"global"}, NOW), false);
  assert.equal(canPerformStaffAction([grant(ROLES.OWNER)], "moderation.role.grant", {type:"room",id:"room-1"}, NOW), false);
});

test("Title validator accepts only explicit boolean visibility preferences", () => {
  const g = [grant(ROLES.DEVELOPER)];
  assert.deepEqual(validateTitlePreference(g, show(TITLES.DEVELOPER), NOW), show(TITLES.DEVELOPER));
  assert.throws(() => validateTitlePreference(g, { ...show(TITLES.DEVELOPER), showOverheadTitle: "true" }, NOW));
  assert.throws(() => validateTitlePreference(g, { ...show(TITLES.DEVELOPER), isAdmin: true }, NOW));
});

test("Having two roles allows selection of one verified label", () => {
  const grants = [grant(ROLES.VOLUNTEER_MOD), grant(ROLES.DEVELOPER)];
  const first = resolvePublicStaffTitle(grants, show(TITLES.VOLUNTEER_MOD), NOW);
  const second = resolvePublicStaffTitle(grants, show(TITLES.DEVELOPER), NOW);
  assert.equal(first.availableTitles.length, 2);
  assert.equal(first.titleText, "Volunteer Mod");
  assert.equal(second.titleText, "Developer");
});
